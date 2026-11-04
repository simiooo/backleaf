//! Explicit read-only CLI probe of the same worker used by GPUI.
#[path="../../../src/search.rs"]
#[allow(dead_code)]
mod search;
mod model { pub enum Event { Search(crate::search::Notice) } }
fn main()->anyhow::Result<()> {
    let mut args=std::env::args_os().skip(1);
    let root=std::path::PathBuf::from(args.next().expect("fixture root"));
    let query=args.next().expect("literal query").to_string_lossy().into_owned();
    let(tx,rx)=async_channel::bounded(8);let worker=search::Worker::new(tx);
    let start=std::time::Instant::now();worker.search(1,root,query);
    let mut first=None;let mut complete=None;let mut peak_page=0;let mut pages=0;let mut final_count=0;
    let deadline=start+std::time::Duration::from_secs(120);
    loop {
        if std::time::Instant::now()>deadline{anyhow::bail!("search timed out");}
        match rx.try_recv() {
            Ok(model::Event::Search(search::Notice::Progress{token:1,count,done,scanned,unavailable,skipped,error}))=>{
                if let Some(error)=error{anyhow::bail!(error);}
                if count>0&&first.is_none(){first=Some(start.elapsed().as_secs_f64());}
                if done&&complete.is_none(){
                    complete=Some((start.elapsed().as_secs_f64(),scanned,unavailable,skipped));final_count=count;
                    worker.page(1,1,0..32);
                }
            },
            Ok(model::Event::Search(search::Notice::Page{token:1,serial,rows}))=>{
                anyhow::ensure!(rows.len()<=32,"page exceeded bound");
                anyhow::ensure!(rows.iter().all(|row|row.path.exists()),"result cannot be located");
                peak_page=peak_page.max(rows.len());pages+=1;
                if serial==1&&final_count>32{worker.page(1,2,final_count.saturating_sub(32)..final_count);}else{break;}
            },
            Ok(_)=>{},
            Err(async_channel::TryRecvError::Empty)=>std::thread::sleep(std::time::Duration::from_millis(2)),
            Err(error)=>return Err(error.into()),
        }
    }
    let(elapsed,scanned,unavailable,skipped)=complete.expect("completed");
    println!("{}",serde_json::json!({"query_kind":"literal_filename","recursive":true,"matched":final_count,"scanned":scanned,"unavailable":unavailable,"links_not_expanded":skipped,"first_result_seconds":first,"complete_seconds":elapsed,"paged_rows_peak":peak_page,"pages_checked":pages,"results_exist":true}));
    Ok(())
}
