//! Isolated viewport/identity/clock model. Does not create a GPUI window.
use std::collections::{HashMap, VecDeque};
use std::hint::black_box;
use std::ops::Range;
use std::time::Instant;

#[derive(Clone, Copy)]
struct Viewport { width: f64, height: f64, pitch: f64, overscan: u64 }
#[derive(Clone, Debug)]
struct WindowRange { visible: Range<u64>, materialised: Range<u64>, columns: u64, scroll: f64 }

impl Viewport {
    fn columns(self) -> u64 { (((self.width-24.).max(0.)+8.)/98.).floor().max(1.) as u64 }
    fn window(self, total: u64, scroll: f64) -> WindowRange {
        let columns=self.columns();
        let rows=total.div_ceil(columns);
        let scroll=scroll.clamp(0., (rows as f64*self.pitch-self.height).max(0.));
        let first=(scroll/self.pitch).floor() as u64;
        let last=(((scroll+self.height)/self.pitch).ceil() as u64).min(rows);
        let start=first.saturating_sub(self.overscan);
        let end=(last+self.overscan).min(rows);
        WindowRange { visible:(first*columns).min(total)..(last*columns).min(total),
            materialised:(start*columns).min(total)..(end*columns).min(total),columns,scroll }
    }
    fn cell_bound(self) -> u64 {
        (self.height/self.pitch).ceil() as u64+1+2*self.overscan
    }
}

struct Cache { names: HashMap<u64,String>, fifo: VecDeque<u64>, capacity: usize, peak: usize }
impl Cache {
    fn new(capacity: usize) -> Self { Self {names:HashMap::new(),fifo:VecDeque::new(),capacity,peak:0} }
    fn bind(&mut self,id:u64) {
        if self.names.contains_key(&id) { return; }
        while self.names.len()>=self.capacity {
            let evicted=self.fifo.pop_front().unwrap();self.names.remove(&evicted);
        }
        self.names.insert(id,format!("资料_{id:012}_项目验收记录_and_a_long_display_filename.pdf"));
        self.fifo.push_back(id); self.peak=self.peak.max(self.names.len());
    }
}

#[derive(Clone,Copy,PartialEq,Eq)]
struct HoverToken { id:u64, generation:u64 }
struct Hover { current:Option<HoverToken>, generation:u64 }
impl Hover {
    fn request(&mut self,id:u64)->HoverToken { let token=HoverToken{id,generation:self.generation};self.current=Some(token);token }
    fn rebind(&mut self) { self.generation+=1;self.current=None; }
    fn accept(&self,token:HoverToken)->bool {self.current==Some(token)&&self.generation==token.generation}
}

fn clock_count(active_visible:usize,hidden:bool,reduced_motion:bool)->usize {
    usize::from(active_visible>0&&!hidden&&!reduced_motion)
}

fn main() {
    let sizes=[1_000_u64,100_000,1_000_000,10_000_000];
    let resolutions=[(560.,720.),(1120.,720.),(1920.,1080.),(3840.,2160.)];
    let scales=[1.,1.25,1.5,2.];
    let mut checked=0_u64;
    let mut maximum_cells=0;
    let mut maximum_cached=0;
    let mut maximum_clock_count=0;
    let mut maximum_animated=0;
    let mut hover=Hover{current:None,generation:0};
    let mut stale_rejected=0;
    for total in sizes {
        for (physical_width,physical_height) in resolutions {
            for scale in scales {
                let view=Viewport {width:physical_width/scale,height:(physical_height/scale-104.).max(100.),pitch:116.,overscan:2};
                let budget=(view.cell_bound()*view.columns()) as usize;
                let mut cache=Cache::new(2*budget+128);
                let mut seed=919_u64;
                for step in 0..256 {
                    seed=seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    let max_scroll=total.div_ceil(view.columns()) as f64*view.pitch;
                    let scroll=match step {0=>0.,1=>max_scroll,2=>max_scroll/2.,_=>((seed>>32) as f64/u32::MAX as f64)*max_scroll};
                    let range=view.window(total,scroll);
                    assert!(range.materialised.start<=range.visible.start&&range.materialised.end>=range.visible.end);
                    assert!(range.materialised.end<=total);
                    let cells=(range.materialised.end-range.materialised.start) as usize;
                    assert!(cells<=budget);
                    maximum_cells=maximum_cells.max(cells);
                    let mut active=0;
                    for id in range.materialised.clone() {
                        cache.bind(id);
                        // Relative coordinates stay small even at deep offsets; don't cast the huge absolute scroll to f32.
                        let y=(id/range.columns) as f64*view.pitch-range.scroll;
                        assert!(y>=-((view.overscan+1) as f64)*view.pitch&&y<=view.height+(view.overscan+1) as f64*view.pitch);
                        if range.visible.contains(&id)&&id%17==0 {active+=1;}
                    }
                    let animated=active.min(8);
                    maximum_animated=maximum_animated.max(animated);
                    let clocks=clock_count(animated,false,false);
                    maximum_clock_count=maximum_clock_count.max(clocks);
                    assert!(clocks<=1);
                    assert_eq!(clock_count(animated,true,false),0);
                    assert_eq!(clock_count(animated,false,true),0);
                    assert_eq!(clock_count(0,false,false),0);
                    assert!(cache.names.len()<=2*budget+128);
                    maximum_cached=maximum_cached.max(cache.peak);
                    if !range.visible.is_empty() {
                        let token=hover.request(range.visible.start);
                        assert!(hover.accept(token));
                        hover.rebind();
                        assert!(!hover.accept(token));
                        stale_rejected+=1;
                    }
                    checked+=1;
                }
            }
        }
    }
    // Constant-time arithmetic timing, independent of metadata storage and actual drawing.
    let view=Viewport{width:1120.,height:616.,pitch:116.,overscan:2};
    let before=Instant::now();
    let mut checksum=0_u64;
    let calculations=1_000_000;
    for i in 0..calculations {
        let range=view.window(black_box(10_000_000),black_box((i%900_000) as f64*100.));
        checksum^=range.materialised.start+range.materialised.end;
    }
    black_box(checksum);
    let elapsed=before.elapsed().as_secs_f64()*1000.;
    let example=view.window(1_000_000,4_210_900.);
    println!("{{\"scope\":\"Isolated Rust viewport, cache, hover identity and animation-clock model; not GPUI rendering\",\"scenarios\":{checked},\"max_materialised_cells\":{maximum_cells},\"max_cached_short_names\":{maximum_cached},\"max_shared_clocks\":{maximum_clock_count},\"max_animated_visible_names\":{maximum_animated},\"stale_hover_results_rejected\":{stale_rejected},\"hidden_and_reduced_motion_clocks\":0,\"example_1120x616_materialised_cells\":{},\"range_calculations\":{calculations},\"range_calculation_ms\":{elapsed:.4},\"not_measured\":[\"GPUI frame time\",\"GPU usage\",\"real glyph layout\",\"real accessibility providers\"]}}",example.materialised.end-example.materialised.start);
}
