//! Opt-in native controller probe. Exercises real GPUI frames, not OS mouse injection.
use crate::{Backleaf, layout, selection::SelectionMask};
use gpui::*;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

fn pointer(
    this: &mut Backleaf,
    window: &mut Window,
    cx: &mut Context<Backleaf>,
    y: f32,
    additive: bool,
) {
    this.move_marquee(
        &MouseMoveEvent {
            position: point(px(f32::from(window.viewport_size().width) * 0.6), px(y)),
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers {
                control: additive,
                ..Default::default()
            },
        },
        window,
        cx,
    );
}
fn sample(this: &Backleaf, window: &Window, phase: &str) -> Result<Value, String> {
    if !window.is_window_active() || this.session_locked {
        return Err("activation-lost-or-locked".into());
    }
    let g = this
        .scroll_geometry(crate::app_scroll::Kind::Files, window)
        .ok_or("no-scroll-range")?;
    let m = this.marquee.as_ref().ok_or("marquee-lost")?;
    if !m.held {
        return Err("marquee-no-longer-held".into());
    }
    Ok(
        json!({"phase":phase,"offset":g.offset,"maximum":g.maximum,"viewport":g.viewport,"velocity":this.marquee_velocity(window),"origin_row":this.origin_row,"band_count":m.band.len(),"commands_blocked":this.selection_busy(),"cache_entries":this.cache.len()}),
    )
}
fn save(
    this: &mut Backleaf,
    cx: &mut Context<Backleaf>,
    result: Result<Value, String>,
    phase: &str,
) {
    let report = match result {
        Ok(mut value) => {
            value["valid"] = json!(true);
            value
        }
        Err(error) => {
            json!({"valid":false,"error":error,"phase":phase,"events":this.metrics.borrow().events})
        }
    };
    if let Some(path) = this.options.output.as_ref() {
        let path = path.with_extension("marquee.json");
        let _ = std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap());
    }
    this.write_report();
    cx.quit();
}
pub(super) fn schedule(window: &Window, cx: &mut Context<Backleaf>) {
    cx.spawn_in(window,async move|this,cx|{
        let mut phase="warmup";
        let result=async{
            cx.background_executor().timer(Duration::from_millis(1500)).await;
            let(count,additive,hold)=this.update_in(cx,|this,window,cx|->Result<_,String>{
                if !window.is_window_active()||this.session_locked{return Err("window-not-active-or-locked".into());}
                let count=this.source.count();let additive=count>=1_000_000;
                this.panel=this.options.flow>0;
                this.selection.select(this.columns-1,false,false);
                window.focus(&this.focus,cx);
                this.begin_marquee(&MouseDownEvent{position:point(px(layout::content_left(this.sidebar_width())-4.),px(layout::content_top()+8.)),button:MouseButton::Left,click_count:1,modifiers:Modifiers{control:additive,..Default::default()},..Default::default()},window,cx);
                pointer(this,window,cx,layout::content_bottom(f32::from(window.viewport_size().height)),additive);
                Ok((count,additive,if count>=1_000_000{28.}else if count>=100_000{18.}else{8.}))
            }).map_err(|_|"window-dropped")??;
            let mut samples=Vec::new();let started=Instant::now();
            phase="deep-hold";
            while started.elapsed().as_secs_f64()<hold{
                cx.background_executor().timer(Duration::from_millis(250)).await;
                let mut value=this.update_in(cx,|this,window,_|sample(this,window,"deep-hold")).map_err(|_|"window-dropped")??;
                value["elapsed"]=json!(started.elapsed().as_secs_f64());samples.push(value);
            }
            phase="return-to-content";
            let(stop,viewport)=this.update_in(cx,|this,window,cx|{
                pointer(this,window,cx,f32::from(window.viewport_size().height)/2.,additive);
                sample(this,window,"return-to-content")
            }).map_err(|_|"window-dropped")??.as_object().map(|s|(s["offset"].as_f64().unwrap(),s["viewport"].as_f64().unwrap())).ok_or("bad-sample")?;
            cx.background_executor().timer(Duration::from_millis(500)).await;
            let stable=this.update_in(cx,|this,window,_|sample(this,window,"stopped")).map_err(|_|"window-dropped")??;
            if (stable["offset"].as_f64().unwrap()-stop).abs()>0.1{return Err("drift-after-return".into());}samples.push(stable);
            phase="reverse";
            this.update_in(cx,|this,window,cx|pointer(this,window,cx,layout::content_top(),additive)).map_err(|_|"window-dropped")?;
            cx.background_executor().timer(Duration::from_millis(200)).await;
            let reverse=this.update_in(cx,|this,window,_|sample(this,window,"reverse-fine")).map_err(|_|"window-dropped")??;
            let reverse_offset=reverse["offset"].as_f64().unwrap();
            if reverse_offset>=stop||stop-reverse_offset>viewport*0.5{return Err("reverse-did-not-restart-fine".into());}samples.push(reverse);
            cx.background_executor().timer(Duration::from_millis(1800)).await;
            phase="release";
            let(band,base)=this.update_in(cx,|this,window,cx|->Result<_,String>{
                pointer(this,window,cx,f32::from(window.viewport_size().height)/2.,additive);
                let m=this.marquee.as_ref().ok_or("marquee-lost")?;let band=m.band;let base=this.columns-1;
                this.update_marquee_band(window,cx);this.finish_marquee(cx);Ok((band,base))
            }).map_err(|_|"window-dropped")??;
            let mut settled:Option<SelectionMask>=None;
            phase="final-mask";
            for _ in 0..100{
                cx.background_executor().timer(Duration::from_millis(100)).await;
                settled=this.update_in(cx,|this,window,_|->Result<_,String>{
                    if !window.is_window_active()||this.session_locked{return Err("activation-lost-or-locked".into());}
                    Ok((this.marquee.is_none()&&!this.selection_busy()).then(||this.selection.mask.clone()))
                }).map_err(|_|"window-dropped")??;
                if settled.is_some(){break;}
            }
            let mask=settled.ok_or("final-mask-timeout")?;
            let expected=band.len()+usize::from(additive&&!band.contains(base));
            if mask.count()!=expected{return Err("final-count-mismatch".into());}
            let serialized=mask.bitmap_serialized_bytes();
            let(tx,rx)=async_channel::bounded(1);
            std::thread::Builder::new().name("backleaf-marquee-verify".into()).spawn(move||{
                let mut visited=0;let mut valid=true;
                for id in mask.ids_from(0,count){if !(band.contains(id)||(additive&&id==base)){valid=false;break;}visited+=1;}
                let _=tx.send_blocking(valid&&visited==expected);
            }).map_err(|_|"verification-worker-failed")?;
            if !rx.recv().await.map_err(|_|"verification-worker-lost")?{return Err("final-identity-mismatch".into());}
            let maximum=samples.first().and_then(|s|s["maximum"].as_f64()).unwrap_or(0.);
            let furthest=samples.iter().filter_map(|s|s["offset"].as_f64()).fold(0.,f64::max);
            if count>=1_000_000&&furthest<maximum*0.95{return Err("large-range-not-reached".into());}
            if samples.iter().any(|s|s.get("commands_blocked")==Some(&json!(false))){return Err("file-commands-not-guarded".into());}
            Ok(json!({"version":env!("CARGO_PKG_VERSION"),"scope":"synthetic controller in real GPUI window; no OS input injected","count":count,"additive":additive,"samples":samples,"stop_pass":true,"reverse_fine_pass":true,"release_pass":true,"final_identity_pass":true,"expected_selected":expected,"selection_bytes":serialized,"maximum_offset":maximum,"furthest_offset":furthest}))
        }.await;
        let _=this.update_in(cx,|this,_,cx|save(this,cx,result,phase));
    }).detach();
}
