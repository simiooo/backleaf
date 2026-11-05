//! Identity remapping and viewport preparation run on the next snapshot's worker.
use super::*;
use crate::selection::{Selection, SelectionMask};
use roaring::RoaringTreemap;

pub struct Request {
    pub old: FrozenSelection,
    pub focus: Option<usize>,
    pub anchor: usize,
    pub top: usize,
    pub columns: usize,
    pub page_len: usize,
    pub cut: Option<PathBuf>,
}
pub struct Ready {
    pub selection: Selection,
    pub top: usize,
    pub range: Range<usize>,
    pub items: Vec<Item>,
}
pub(super) fn prepare(
    db: &Connection,
    snapshot: &DirectorySnapshot,
    request: Request,
    stop: &AtomicBool,
) -> anyhow::Result<Ready> {
    let old = Connection::open_with_flags(
        &request.old.snapshot.database,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    // The parent itself may have been replaced. Never carry a selection into that directory.
    let same_parent = snapshot.parent_identity.get().is_some()
        && snapshot.parent_identity.get() == request.old.snapshot.parent_identity.get();
    let mut exact = db.prepare_cached("SELECT id FROM files WHERE identity=?1 AND raw=?2")?;
    let mut by_identity = db.prepare_cached("SELECT id FROM files WHERE identity=?1 LIMIT 2")?;
    let mut old_identity = old.prepare_cached("SELECT id FROM files WHERE identity=?1 LIMIT 2")?;
    let mut old_row = old.prepare_cached("SELECT raw,identity FROM files WHERE id=?1")?;
    let mut map = |id: usize| -> anyhow::Result<Option<usize>> {
        if !same_parent || stop.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let value = old_row
            .query_row([id as i64], |r| {
                Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Vec<u8>>(1)?))
            })
            .optional()?;
        let Some((raw, identity)) = value else {
            return Ok(None);
        };
        if let Some(id) = exact
            .query_row(params![identity, raw], |r| r.get::<_, i64>(0))
            .optional()?
        {
            return Ok(Some(id as usize));
        }
        // Hard links share identity; a rename is safe only when unique in both snapshots.
        let next = by_identity
            .query_map([&identity], |r| r.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let previous = old_identity
            .query_map([&identity], |r| r.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok((next.len() == 1 && previous.len() == 1).then(|| next[0] as usize))
    };
    let focus = request.focus.map(&mut map).transpose()?.flatten();
    let anchor = map(request.anchor)?.or(focus).unwrap_or(0);
    let top = map(request.top)?.unwrap_or(request.top);
    let mut selected = RoaringTreemap::new();
    let mut matched = RoaringTreemap::new();
    if same_parent
        && request.old.mask.count() > 32
        && request.old.mask.count() > request.old.count / 8
    {
        // Bulk exact matches use one indexed join instead of two SQL statements per
        // selected file. Sparse exceptions/renames still use the unambiguous mapping.
        db.execute(
            "ATTACH DATABASE ?1 AS previous",
            [request.old.snapshot.database.to_string_lossy().as_ref()],
        )?;
        struct Attached<'a>(&'a Connection);
        impl Drop for Attached<'_> {
            fn drop(&mut self) {
                let _ = self.0.execute_batch("DETACH DATABASE previous");
            }
        }
        let _attached = Attached(db);
        let mut query=db.prepare("SELECT old.id,next.id FROM previous.files old JOIN main.files next ON next.identity=old.identity AND next.raw=old.raw ORDER BY old.id")?;
        let mut rows = query.query([])?;
        while let Some(row) = rows.next()? {
            anyhow::ensure!(!stop.load(Ordering::Relaxed), "refresh cancelled");
            let id = row.get::<_, i64>(0)? as usize;
            if request.old.mask.contains(id) {
                matched.insert(id as u64);
                selected.insert(row.get::<_, i64>(1)? as u64);
            }
        }
    }
    for id in request.old.mask.ids_from(0, request.old.count) {
        anyhow::ensure!(!stop.load(Ordering::Relaxed), "refresh cancelled");
        if matched.contains(id as u64) {
            continue;
        }
        if let Some(id) = map(id)? {
            selected.insert(id as u64);
        }
    }
    let count = db.query_row("SELECT COUNT(*) FROM files", [], |r| r.get::<_, i64>(0))? as usize;
    let top = top.min(count.saturating_sub(1));
    let columns = request.columns.max(1);
    let start = (top / columns).saturating_sub(2) * columns;
    let range = start
        ..(start
            + request
                .page_len
                .clamp(columns.saturating_mul(12).min(4096), 4096))
        .min(count);
    let cut_db = request.cut.as_ref().and_then(|p| {
        Connection::open_with_flags(p, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()
    });
    let mut query = db.prepare_cached(
        "SELECT id,short,dir,bytes,raw FROM files WHERE id>=?1 AND id<?2 ORDER BY id",
    )?;
    let items = query
        .query_map(params![range.start as i64, range.end as i64], |r| {
            let name: String = r.get(1)?;
            let raw: Vec<u8> = r.get(4)?;
            let os_name = decode_name(&raw);
            let cut = cut_db
                .as_ref()
                .is_some_and(|db| cut_matches(db, &snapshot.parent.get().unwrap().join(&os_name)));
            Ok(Item {
                id: r.get::<_, i64>(0)? as usize,
                label: short_name(&name, 96),
                name,
                os_name,
                directory: r.get(2)?,
                bytes: r.get::<_, Option<i64>>(3)?.map(|v| v as u64),
                cut,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Ready {
        selection: Selection {
            mask: SelectionMask::from_bitmap(selected),
            focus,
            anchor,
        },
        top,
        range,
        items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot(
        path: PathBuf,
        count: usize,
        reverse: bool,
    ) -> (Arc<DirectorySnapshot>, Connection) {
        let snapshot = Arc::new(DirectorySnapshot {
            database: path,
            parent: OnceLock::new(),
            parent_identity: OnceLock::new(),
        });
        snapshot
            .parent
            .set(PathBuf::from("C:\\refresh-fixture"))
            .unwrap();
        snapshot.parent_identity.set([1; 33]).unwrap();
        let mut db = Connection::open(&snapshot.database).unwrap();
        db.execute_batch("PRAGMA journal_mode=MEMORY; PRAGMA synchronous=OFF; CREATE TABLE files(id INTEGER PRIMARY KEY,short TEXT,raw BLOB,dir INTEGER,bytes INTEGER,identity BLOB); CREATE INDEX files_identity ON files(identity,raw);").unwrap();
        let transaction = db.transaction().unwrap();
        {
            let mut insert = transaction
                .prepare("INSERT INTO files VALUES(?1,?2,?3,0,4096,?4)")
                .unwrap();
            for i in 0..count {
                let name = format!("file-{i:06}.txt");
                let mut identity = [0u8; 33];
                identity[..8].copy_from_slice(&(i as u64).to_le_bytes());
                insert
                    .execute(params![
                        if reverse { count - 1 - i } else { i } as i64,
                        name,
                        encode_name(std::ffi::OsStr::new(&name)),
                        identity.as_slice()
                    ])
                    .unwrap();
            }
        }
        transaction.commit().unwrap();
        (snapshot, db)
    }
    #[test]
    fn hundred_thousand_selected_rows_remap_on_worker_with_bounded_viewport() {
        let fixture = crate::test_support::Fixture::new("refresh-large");
        let (old, old_db) = snapshot(fixture.0.join("old.sqlite"), 100_000, false);
        let (next, db) = snapshot(fixture.0.join("next.sqlite"), 100_000, true);
        let mut selection = Selection::default();
        selection.all(100_000);
        let started = std::time::Instant::now();
        let ready = prepare(
            &db,
            &next,
            Request {
                old: FrozenSelection {
                    snapshot: old,
                    count: 100_000,
                    mask: selection.mask,
                },
                focus: Some(7),
                anchor: 7,
                top: 50_000,
                columns: 8,
                page_len: 96,
                cut: None,
            },
            &AtomicBool::new(false),
        )
        .unwrap();
        println!(
            "100000 identity remaps: {:?}; viewport={}, bitmap={} bytes",
            started.elapsed(),
            ready.items.len(),
            ready.selection.mask.bitmap_serialized_bytes()
        );
        assert_eq!(ready.selection.count(), 100_000);
        assert_eq!(ready.selection.focus, Some(99_992));
        assert_eq!(ready.top, 49_999);
        assert!(ready.items.len() <= 96);
        assert!(ready.selection.mask.bitmap_serialized_bytes() < 8192);
        drop(old_db);
        drop(db);
    }
    #[test]
    fn ambiguous_hard_link_renames_do_not_inherit_selection_and_cancel_is_observed() {
        let fixture = crate::test_support::Fixture::new("refresh-hardlinks");
        let (old, old_db) = snapshot(fixture.0.join("old.sqlite"), 2, false);
        old_db
            .execute("UPDATE files SET identity=zeroblob(33)", [])
            .unwrap();
        let (next, db) = snapshot(fixture.0.join("next.sqlite"), 1, false);
        db.execute(
            "UPDATE files SET identity=zeroblob(33),raw=?1",
            [encode_name(std::ffi::OsStr::new("renamed.txt"))],
        )
        .unwrap();
        let mut selection = Selection::default();
        selection.all(2);
        let request = || Request {
            old: FrozenSelection {
                snapshot: old.clone(),
                count: 2,
                mask: selection.mask.clone(),
            },
            focus: Some(0),
            anchor: 0,
            top: 0,
            columns: 1,
            page_len: 32,
            cut: None,
        };
        assert_eq!(
            prepare(&db, &next, request(), &AtomicBool::new(false))
                .unwrap()
                .selection
                .count(),
            0
        );
        assert!(prepare(&db, &next, request(), &AtomicBool::new(true)).is_err());
        drop(old_db);
        drop(db);
    }
}
