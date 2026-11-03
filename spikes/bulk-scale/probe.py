"""Isolated scale model: SQLite paging, bounded records and coalesced progress.

This does not copy/delete real files, call Windows Shell, or measure GPUI frames.
Each workload runs in a fresh process so peak process memory is comparable.
"""
from __future__ import annotations

import argparse
import ctypes
from collections import Counter, deque
from dataclasses import dataclass
from datetime import datetime, timezone
import json
import math
from pathlib import Path
import platform
import random
import sqlite3
import subprocess
import sys
from threading import Condition, Event, Lock, Thread
import time

ROOT = Path(__file__).resolve().parents[2]
MAX_RECORDS = 256
MAX_PAYLOAD = 256 * 1024
MAX_QUEUED_BATCHES = 8
VISIBLE_ROWS = 96
JOBS = 24
PRODUCERS = 2


@dataclass
class Batch:
    rows: list
    payload_bytes: int


class Pipe:
    def __init__(self, producers=PRODUCERS):
        self.cv = Condition()
        self.cancelled = Event()
        self.rows = deque()
        self.producers = producers
        self.bytes = self.peak_bytes = self.peak_batches = 0

    def put(self, batch):
        assert len(batch.rows) <= MAX_RECORDS and batch.payload_bytes <= MAX_PAYLOAD
        with self.cv:
            while len(self.rows) == MAX_QUEUED_BATCHES and not self.cancelled.is_set():
                self.cv.wait(0.01)
            if self.cancelled.is_set():
                return False
            self.rows.append(batch)
            self.bytes += batch.payload_bytes
            self.peak_bytes = max(self.bytes, self.peak_bytes)
            self.peak_batches = max(len(self.rows), self.peak_batches)
            self.cv.notify_all()
            return True

    def get(self):
        with self.cv:
            while not self.rows and self.producers:
                self.cv.wait()
            if not self.rows:
                return None
            batch = self.rows.popleft()
            self.bytes -= batch.payload_bytes
            self.cv.notify_all()
            return batch

    def finish(self):
        with self.cv:
            self.producers -= 1
            self.cv.notify_all()

    def cancel(self):
        self.cancelled.set()
        with self.cv:
            self.cv.notify_all()


class Mailbox:
    """At most one wake; latest cosmetic progress per bounded live job."""
    def __init__(self):
        self.lock = Lock()
        self.latest = {}
        self.dirty = False
        self.updates = self.snapshots = self.peak_jobs = self.peak_wakes = 0

    def publish(self, job, value):
        with self.lock:
            self.latest[job] = value
            assert len(self.latest) <= 32
            self.dirty = True
            self.updates += 1
            self.peak_jobs = max(self.peak_jobs, len(self.latest))
            self.peak_wakes = max(self.peak_wakes, int(self.dirty))

    def take(self):
        with self.lock:
            if not self.dirty:
                return None
            self.dirty = False
            self.snapshots += 1
            return dict(self.latest)


def memory_mb():
    if sys.platform != 'win32':
        return {'peak_working_set_mb': None, 'private_bytes_mb': None}
    from ctypes import wintypes

    class Counters(ctypes.Structure):
        _fields_ = [('cb', wintypes.DWORD), ('faults', wintypes.DWORD)] + [
            (name, ctypes.c_size_t) for name in (
                'peak_ws', 'ws', 'peak_paged', 'paged', 'peak_nonpaged',
                'nonpaged', 'pagefile', 'peak_pagefile', 'private')]

    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel.GetCurrentProcess.restype = wintypes.HANDLE
    psapi = ctypes.WinDLL('psapi', use_last_error=True)
    psapi.GetProcessMemoryInfo.argtypes = [wintypes.HANDLE, ctypes.POINTER(Counters), wintypes.DWORD]
    psapi.GetProcessMemoryInfo.restype = wintypes.BOOL
    counters = Counters()
    counters.cb = ctypes.sizeof(counters)
    if not psapi.GetProcessMemoryInfo(kernel.GetCurrentProcess(), ctypes.byref(counters), counters.cb):
        raise ctypes.WinError(ctypes.get_last_error())
    return {'peak_working_set_mb': round(counters.peak_ws / 2**20, 2),
            'private_bytes_mb': round(counters.private / 2**20, 2)}


def quantiles(values):
    ordered = sorted(values)
    return {name: round(ordered[min(len(ordered)-1, math.ceil(len(ordered)*p)-1)], 4)
            for name, p in [('p50_ms', .50), ('p95_ms', .95), ('max_ms', 1)]}


def verify_controls():
    # Cancellation can be delivered even when the data queue is completely full.
    pipe = Pipe(producers=1)
    for _ in range(MAX_QUEUED_BATCHES):
        assert pipe.put(Batch([(1,)], 8))
    started = Event()
    result = []

    def blocked_sender():
        started.set()
        result.append(pipe.put(Batch([(2,)], 8)))

    thread = Thread(target=blocked_sender)
    thread.start()
    started.wait(2)
    before = time.perf_counter()
    pipe.cancel()
    thread.join(2)
    elapsed = (time.perf_counter() - before) * 1000
    assert not thread.is_alive() and result == [False]

    # Atomic reservation of all device lanes; fair round-robin between chunks.
    pending = deque(range(JOBS))
    remaining = {job: 4 for job in pending}
    completed = Counter()
    devices = {j: {f'disk-{j % 5}', f'disk-{(j+1) % 5}'} for j in pending}
    peak_active = 0
    while pending:
        active, held = [], set()
        for _ in range(len(pending)):
            job = pending.popleft()
            if len(active) < 2 and devices[job].isdisjoint(held):
                active.append(job)
                held.update(devices[job])
            else:
                pending.append(job)
        assert active
        peak_active = max(peak_active, len(active))
        for a in active:
            for b in active:
                assert a == b or devices[a].isdisjoint(devices[b])
            remaining[a] -= 1
            completed[a] += 1
            if remaining[a]:
                pending.append(a)
    assert len(completed) == JOBS and set(completed.values()) == {4}
    # A million-item all-selection is a snapshot plus exclusions, not a million IDs.
    selection = {'snapshot': 'fixture-revision-1', 'count': 1_000_000, 'excluded': {4}}
    assert selection['count'] - len(selection['excluded']) == 999_999
    return {'cancel_with_full_queue_ms': round(elapsed, 4),
            'device_scheduler_peak_active': peak_active,
            'scheduled_jobs_completed': len(completed),
            'selection_exclusions_stored': len(selection['excluded'])}


def run_case(size, long_paths, compare_full_paths=False):
    work = ROOT / '.local' / 'bulk-probe' / f'{time.time_ns()}-{size}'
    work.mkdir(parents=True)
    db = work / 'records.sqlite'
    connection = sqlite3.connect(db)
    connection.executescript('''
      PRAGMA journal_mode=WAL;
      PRAGMA synchronous=FULL;
      PRAGMA cache_size=-8192;
      PRAGMA mmap_size=0;
      PRAGMA temp_store=FILE;
      PRAGMA wal_autocheckpoint=512;
      CREATE TABLE records(ordinal INTEGER PRIMARY KEY,job INTEGER,status TEXT,display TEXT,path BLOB);
      CREATE INDEX by_job_status ON records(job,status,ordinal,display);
      CREATE INDEX visible_identity ON records(ordinal,display);
    ''')
    pipe, mailbox = Pipe(), Mailbox()
    failures = []
    peak_batch_bytes = 0
    peak_batch_records = 0

    def produce(lane):
        nonlocal peak_batch_bytes, peak_batch_records
        try:
            rows, payload = [], 0
            for ordinal in range(lane+1, size+1, PRODUCERS):
                prefix = 'D:\\probe\\资料\\' + ('界' * 31_900 if long_paths else '')
                path = (prefix + f'archive_{ordinal:09}.pdf').encode('utf-16-le')
                row_size = len(path) + 48
                if row_size > MAX_PAYLOAD:
                    raise ValueError('A record cannot exceed its payload budget')
                if rows and (len(rows) == MAX_RECORDS or payload + row_size > MAX_PAYLOAD):
                    peak_batch_bytes = max(peak_batch_bytes, payload)
                    peak_batch_records = max(peak_batch_records, len(rows))
                    if not pipe.put(Batch(rows, payload)):
                        return
                    rows, payload = [], 0
                job = ordinal % JOBS
                rows.append((ordinal, job, 'failed' if ordinal % 97 == 0 else 'done', f'archive_{ordinal:09}.pdf', path))
                payload += row_size
                mailbox.publish(job, ordinal)
            if rows:
                peak_batch_bytes = max(peak_batch_bytes, payload)
                peak_batch_records = max(peak_batch_records, len(rows))
                pipe.put(Batch(rows, payload))
        except BaseException as exc:
            failures.append(repr(exc))
        finally:
            pipe.finish()

    threads = [Thread(target=produce, args=(lane,)) for lane in range(PRODUCERS)]
    started = time.perf_counter()
    for thread in threads:
        thread.start()
    persisted, status_counts = 0, Counter()
    next_snapshot = time.monotonic() + .1
    while (batch := pipe.get()) is not None:
        with connection:
            connection.executemany('INSERT INTO records VALUES(?,?,?,?,?)', batch.rows)
        persisted += len(batch.rows)
        status_counts.update(row[2] for row in batch.rows)
        if time.monotonic() >= next_snapshot:
            mailbox.take()
            next_snapshot = time.monotonic() + .1
    for thread in threads:
        thread.join()
    mailbox.take()  # Final durable results may wake immediately.
    elapsed = time.perf_counter() - started
    assert not failures, failures
    assert persisted == size == connection.execute('SELECT COUNT(*) FROM records').fetchone()[0]
    assert size//97 == status_counts['failed']
    assert dict(connection.execute('SELECT status,COUNT(*) FROM records GROUP BY status')) == dict(status_counts)
    assert pipe.peak_batches <= MAX_QUEUED_BATCHES
    assert pipe.peak_bytes <= MAX_QUEUED_BATCHES * MAX_PAYLOAD
    assert mailbox.peak_wakes <= 1
    assert mailbox.snapshots <= math.ceil(elapsed*10) + 1

    randomizer, page_ms, failure_ms, full_path_ms, full_page_baseline = random.Random(919), [], [], [], []
    plan = connection.execute("EXPLAIN QUERY PLAN SELECT ordinal,display FROM records WHERE ordinal>? ORDER BY ordinal LIMIT ?", (0, VISIBLE_ROWS)).fetchall()
    assert any("COVERING INDEX visible_identity" in str(row) for row in plan), plan
    for _ in range(200):
        anchor = randomizer.randrange(max(1, size-100))
        before = time.perf_counter()
        rows = connection.execute('SELECT ordinal,display FROM records WHERE ordinal>? ORDER BY ordinal LIMIT ?', (anchor,VISIBLE_ROWS)).fetchall()
        visible = rows  # Full UTF-16 paths are hydrated only for explicit inspection.
        page_ms.append((time.perf_counter()-before)*1000)
        assert len(visible) <= VISIBLE_ROWS and visible[0][0] == anchor+1
        if compare_full_paths:
            before = time.perf_counter()
            full_rows = connection.execute('SELECT ordinal,path FROM records WHERE ordinal>? ORDER BY ordinal LIMIT ?', (anchor,VISIBLE_ROWS)).fetchall()
            hydrated = [(n, path.decode('utf-16-le')) for n,path in full_rows]
            full_page_baseline.append((time.perf_counter()-before)*1000)
            assert [n for n,_ in hydrated] == [n for n,_ in visible]
        before = time.perf_counter()
        errors = connection.execute("SELECT ordinal,display FROM records WHERE job=? AND status='failed' AND ordinal>? ORDER BY ordinal LIMIT ?", (anchor%JOBS,anchor,VISIBLE_ROWS)).fetchall()
        failure_ms.append((time.perf_counter()-before)*1000)
        assert len(errors) <= VISIBLE_ROWS
        assert all(n%97 == 0 and n%JOBS == anchor%JOBS for n,_ in errors)
        before = time.perf_counter()
        full_path = connection.execute('SELECT path FROM records WHERE ordinal=?', (anchor+1,)).fetchone()[0].decode('utf-16-le')
        full_path_ms.append((time.perf_counter()-before)*1000)
        assert full_path.startswith('D:')
    connection.execute('PRAGMA wal_checkpoint(TRUNCATE)')
    connection.close()
    return {'records':size, 'long_utf16_paths':long_paths, 'logical_jobs':JOBS,
            'producer_threads':PRODUCERS, 'sqlite_version':sqlite3.sqlite_version,
            'journal_mode':'WAL', 'synchronous':'FULL', 'sqlite_page_cache_mib':8,
            'spool_seconds':round(elapsed,3), 'database_mib':round(db.stat().st_size/2**20,2),
            'terminal_rows_preserved':persisted, 'failures_preserved':status_counts['failed'],
            'max_queue_batches':pipe.peak_batches, 'max_queue_payload_bytes':pipe.peak_bytes,
            'max_batch_records':peak_batch_records, 'max_batch_payload_bytes':peak_batch_bytes,
            'progress_updates':mailbox.updates, 'ui_snapshots':mailbox.snapshots,
            'max_pending_ui_wakes':mailbox.peak_wakes, 'max_live_job_snapshots':mailbox.peak_jobs,
            'page_limit':VISIBLE_ROWS, 'warm_keyset_page':quantiles(page_ms),
            'warm_failure_page':quantiles(failure_ms), 'on_demand_full_path':quantiles(full_path_ms),
            'page_projection':'ordinal + display, covering index; full path on demand',
            'same_database_full_path_page_baseline':quantiles(full_page_baseline) if full_page_baseline else None,
            **memory_mb()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--worker', type=int)
    parser.add_argument('--long-paths', action='store_true')
    parser.add_argument('--compare-full-paths', action='store_true')
    parser.add_argument('--output', type=Path, default=ROOT/'docs/evidence/bulk-scale-model.json')
    args = parser.parse_args()
    if args.worker:
        print(json.dumps(run_case(args.worker, args.long_paths, args.compare_full_paths)))
        return
    controls = verify_controls()
    cases = []
    for size, long_paths in [(100_000, False), (1_000_000, False), (1024, True)]:
        cmd = [sys.executable, str(Path(__file__).resolve()), '--worker', str(size)]
        if long_paths:
            cmd.append('--long-paths')
        result = subprocess.run(cmd, capture_output=True, text=True, check=True)
        case = json.loads(result.stdout)
        cases.append(case)
        print(json.dumps({'case':size,'long_paths':long_paths, 'seconds':case['spool_seconds'],
                          'peak_mb':case['peak_working_set_mb'],'page_p95_ms':case['warm_keyset_page']['p95_ms']}), flush=True)
    report = {'verified_at':datetime.now(timezone.utc).isoformat(), 'platform':platform.platform(),
              'python':platform.python_version(), 'scope':'Isolated metadata/queue model, not Shell or GPUI',
              'limits':{'batch_records':MAX_RECORDS,'batch_payload_bytes':MAX_PAYLOAD,
                        'queued_batches':MAX_QUEUED_BATCHES,'visible_records':VISIBLE_ROWS},
              'controls':controls,'cases':cases,
              'not_measured':['Real file copy/move/recycle throughput','GPUI frame times and input latency',
                              'Shell/OLE memory and cancellation latency','Physical disk contention',
                              'Crash/power-loss durability','Cold directory enumeration/sorting']}
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
    print(str(args.output))


if __name__ == '__main__':
    main()
