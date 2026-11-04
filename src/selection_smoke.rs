//! Opt-in controller integration in a real GPUI window. It does not inject OS input.
use crate::Backleaf;
use gpui::*;
use std::time::Duration;

fn begin(
    this: &mut Backleaf,
    window: &mut Window,
    cx: &mut Context<Backleaf>,
    additive: bool,
    bottom: f32,
) {
    let modifiers = Modifiers {
        control: additive,
        ..Default::default()
    };
    this.begin_marquee(
        &MouseDownEvent {
            position: point(px(6.), px(54.)),
            button: MouseButton::Left,
            modifiers,
            click_count: 1,
            ..Default::default()
        },
        window,
        cx,
    );
    this.move_marquee(
        &MouseMoveEvent {
            position: point(px(330.), px(bottom)),
            pressed_button: Some(MouseButton::Left),
            modifiers,
        },
        window,
        cx,
    );
}
pub(super) fn schedule(window: &Window, cx: &mut Context<Backleaf>) {
    cx.spawn_in(window,async move|this,cx|{
        cx.background_executor().timer(Duration::from_millis(1500)).await;
        if this.update_in(cx,|this,window,cx|{
            this.panel=false;this.selection.select(3,false,false);
            window.focus(&this.focus,cx);
            this.key(&KeyDownEvent{keystroke:Keystroke::parse("alt-space").unwrap(),is_held:false,prefer_character_input:false},window,cx);
            let preserved=this.selection.count()==1&&this.selection.contains(3);
            this.metrics.borrow_mut().event(&format!("selection-smoke system-shortcut-pass={preserved}"));
            this.scroll_global(3966,ScrollStrategy::Top,true);
            this.metrics.borrow_mut().event("selection-smoke scope=synthetic-controller-events-real-window-not-OS-input");
            cx.notify();
        }).is_err(){return;}
        cx.background_executor().timer(Duration::from_millis(200)).await;
        if this.update_in(cx,|this,window,cx|{
            let bottom=f32::from(window.viewport_size().height)-30.;
            begin(this,window,cx,false,bottom);
        }).is_err(){return;}
        cx.background_executor().timer(Duration::from_millis(850)).await;
        if this.update_in(cx,|this,window,cx|{
            let offset=-f32::from(this.scroll.0.borrow().base_handle.offset().y);
            let moved=this.global_point(point(px(12.),px(60.))).1>3966.*crate::layout::row_step() as f64+100.;
            let rebased=this.origin_row>0;
            this.metrics.borrow_mut().event(&format!("selection-smoke edge-pass={moved} rebase-pass={rebased} offset={offset:.2} origin={}",this.origin_row));
            this.key(&KeyDownEvent{keystroke:Keystroke::parse("escape").unwrap(),is_held:false,prefer_character_input:false},window,cx);
        }).is_err(){return;}
        cx.background_executor().timer(Duration::from_millis(150)).await;
        let expected=this.update_in(cx,|this,window,cx|{
            let restored=this.marquee.is_none()&&this.selection.count()==1&&this.selection.contains(3);
            this.metrics.borrow_mut().event(&format!("selection-smoke escape-pass={restored}"));
            begin(this,window,cx,true,280.);
            let expected=this.marquee.as_ref().map(|m|m.band.len()+1).unwrap_or(0);
            this.finish_marquee(cx);expected
        });
        let Ok(expected)=expected else{return;};
        cx.background_executor().timer(Duration::from_millis(250)).await;
        let _=this.update_in(cx,|this,_,cx|{
            let committed=this.marquee.is_none()&&this.selection.count()==expected&&this.selection.contains(3)&&expected>1;
            this.metrics.borrow_mut().event(&format!("selection-smoke additive-pass={committed} expected={expected} selected={}",this.selection.count()));
            cx.notify();
        });
    }).detach();
}
