use materialcraft::json::Json;
use materialcraft::session::Session;
type P=&'static [(&'static str,f64)];
struct Acc{k:&'static str,p:P,lo:f64,hi:f64,col:&'static str,op:f64}
struct M{
    name:&'static str, base:&'static str, bp:P,
    detail:Option<(&'static str,P,i64,f64)>, // kind, params, blend mode, opacity
    blur:f64, steps:i64, lv:(f64,f64), inv:bool,
    cols:&'static [(f64,&'static str)], acc:Option<Acc>,
    rough:(f64,f64), metal:f64, metal_hi:bool, nrm:f64, ao:bool,
}
fn hex(h:&str)->[f64;4]{ let v=|i:usize|u8::from_str_radix(&h[i..i+2],16).unwrap() as f64/255.0; [v(1),v(3),v(5),1.0] }
fn pj(p:&[(&str,f64)])->Json{ let mut o=Json::obj(); for (k,v) in p{ o=o.with(k,*v);} o }
fn run(s:&mut Session,c:&str,p:Json)->Json{ match s.execute(c,&p){Ok(o)=>o.data,Err(e)=>panic!("{c}: {e}")} }
fn lib(k:&str)->String{ format!("sbs:lib:{k}") }
fn add(s:&mut Session,kind:&str,name:&str,p:Json,i:Json){ run(s,"node.add",Json::obj().with("kind",kind).with("name",name).with("params",p).with("inputs",i)); }
fn arr(v:f64)->Json{ Json::parse(&format!("[{v},{v},{v},{v}]")).unwrap() }
fn build(m:&M,size:i64,out:&str)->Session{
    let mut s=Session::new();
    run(&mut s,"graph.new",Json::obj().with("template","material").with("size",size as i32).with("name",m.name));
    add(&mut s,&lib(m.base),"base",pj(m.bp),Json::obj());
    let mut h="base".to_string();
    if let Some((k,p,mode,op))=&m.detail{
        add(&mut s,&lib(k),"det",pj(p),Json::obj());
        add(&mut s,"sbs:blend","dmix",Json::obj().with("blendingmode",*mode as i32).with("opacitymult",*op),Json::obj().with("source",Json::from("det")).with("destination",Json::from(h.as_str())));
        h="dmix".into();
    }
    if m.blur>0.0{ add(&mut s,"sbs:blur","bl",Json::obj().with("intensity",m.blur),Json::obj().with("input1",Json::from(h.as_str()))); h="bl".into(); }
    let (lo,hi)=m.lv;
    let (o0,o1)=if m.inv{(1.0,0.0)}else{(0.0,1.0)};
    add(&mut s,"sbs:levels","lvh",Json::obj().with("levelinlow",arr(lo)).with("levelinhigh",arr(hi)).with("leveloutlow",arr(o0)).with("levelouthigh",arr(o1)),Json::obj().with("input1",Json::from(h.as_str())));
    h="lvh".into();
    if m.steps>0{ add(&mut s,&lib("quantize_2/quantize_grayscale"),"q",Json::obj().with("steps",m.steps as i32),Json::obj().with("input",Json::from(h.as_str()))); h="q".into(); }
    // colour
    let keys:Vec<String>=m.cols.iter().map(|(p,c)|{let c=hex(c);format!(r#"{{"position":{p},"color":[{},{},{},1]}}"#,c[0],c[1],c[2])}).collect();
    add(&mut s,"sbs:gradient","col",Json::obj(),Json::obj().with("input1",Json::from(h.as_str())));
    run(&mut s,"node.set",Json::obj().with("node","col").with("params",Json::obj().with("gradient_keys",Json::parse(&format!("[{}]",keys.join(","))).unwrap())));
    let mut colnode="col".to_string();
    if let Some(a)=&m.acc{
        add(&mut s,&lib(a.k),"am",pj(a.p),Json::obj());
        add(&mut s,"sbs:levels","amask",Json::obj().with("levelinlow",arr(a.lo)).with("levelinhigh",arr(a.hi)),Json::obj().with("input1",Json::from("am")));
        let c=hex(a.col);
        add(&mut s,"sbs:uniform","acol",Json::obj().with("outputcolor",Json::parse(&format!("[{},{},{},1]",c[0],c[1],c[2])).unwrap()),Json::obj());
        add(&mut s,"sbs:blend","amix",Json::obj().with("opacitymult",a.op),Json::obj().with("source",Json::from("acol")).with("destination",Json::from("col")).with("opacity",Json::from("amask")));
        colnode="amix".into();
    }
    add(&mut s,"sbs:normal","nrm",Json::obj().with("intensity",m.nrm),Json::obj().with("input1",Json::from(h.as_str())));
    add(&mut s,"sbs:levels","rough",Json::obj().with("leveloutlow",arr(m.rough.0)).with("levelouthigh",arr(m.rough.1)),Json::obj().with("input1",Json::from(h.as_str())));
    let mnode=if m.metal_hi{ add(&mut s,"sbs:levels","met",Json::obj().with("levelinlow",arr(0.45)).with("levelinhigh",arr(0.55)).with("leveloutlow",arr(0.0)).with("levelouthigh",arr(m.metal)),Json::obj().with("input1",Json::from(h.as_str()))); "met" }
      else { let v=m.metal; add(&mut s,"sbs:uniform","met",Json::obj().with("colorswitch",0).with("outputcolor",Json::parse(&format!("[{v},{v},{v},1]")).unwrap()),Json::obj()); "met" };
    let aon=if m.ao{ add(&mut s,&lib("hbao_2/hbao"),"ao",Json::obj(),Json::obj().with("input",Json::from(h.as_str()))); "ao" } else { add(&mut s,"sbs:uniform","ao",Json::obj().with("colorswitch",0).with("outputcolor",Json::parse("[1,1,1,1]").unwrap()),Json::obj()); "ao" };
    let conn=|s:&mut Session,f:&str,t:u32|{ run(s,"node.connect",Json::obj().with("from",f).with("to",t).with("slot","inputNodeOutput")); };
    conn(&mut s,&colnode,1); conn(&mut s,"nrm",3); conn(&mut s,"rough",5); conn(&mut s,mnode,7); conn(&mut s,&h,9); conn(&mut s,aon,11);
    for id in [2,4,6,8,10,12]{ let _=s.execute("node.remove",&Json::obj().with("node",id as i32)); }
    run(&mut s,"graph.save",Json::obj().with("path",out));
    s
}
macro_rules! m{($($k:ident:$v:expr),*)=>{M{$($k:$v),*}}}
fn mats()->Vec<M>{ vec![
 m!{name:"cobblestone",base:"pattern_tile_random_2/tile_random_2",bp:&[("amount_x",7.0),("amount_y",7.0),("interstice_thickness",0.14),("round_corners_radius",0.6),("random_size_multiplier",0.3)],detail:Some(("noise_clouds_2/clouds_2",&[("scale",4.0)],3,0.5)),blur:4.0,steps:0,lv:(0.1,0.9),inv:false,cols:&[(0.0,"#2a2833"),(0.3,"#4b4a5c"),(0.65,"#7d7b8f"),(1.0,"#b4b2c4")],acc:None,rough:(0.55,0.95),metal:0.0,metal_hi:false,nrm:2.5,ao:true},
 m!{name:"brick_wall",base:"pattern_brick_generator/brick_generator",bp:&[("Keep_Ratio",1.0)],detail:Some(("noise_grunge_map_001/grunge_map_001",&[],3,0.35)),blur:2.0,steps:0,lv:(0.0,1.0),inv:false,cols:&[(0.0,"#3b2a2a"),(0.35,"#a2503a"),(0.7,"#c96a45"),(1.0,"#e89a6b")],acc:None,rough:(0.6,0.95),metal:0.0,metal_hi:false,nrm:2.0,ao:true},
 m!{name:"stone_tiles",base:"pattern_tile_generator/tile_generator",bp:&[("x_amount",5.0),("y_amount",5.0),("interstice",0.04)],detail:Some(("noise_cells_1/cells_1",&[("scale",12.0)],3,0.25)),blur:2.0,steps:4,lv:(0.0,1.0),inv:false,cols:&[(0.0,"#3a4452"),(0.5,"#6f8296"),(1.0,"#a9bccf")],acc:None,rough:(0.5,0.85),metal:0.0,metal_hi:false,nrm:2.0,ao:true},
 m!{name:"mossy_rock",base:"noise_cells_4/cells_4",bp:&[("scale",6.0),("disorder",0.6)],detail:Some(("noise_clouds_2/clouds_2",&[("scale",4.0)],0,0.4)),blur:3.0,steps:5,lv:(0.0,1.0),inv:false,cols:&[(0.0,"#34323c"),(0.5,"#625f6e"),(1.0,"#928fa0")],acc:Some(Acc{k:"noise_perlin_noise/perlin_noise",p:&[("scale",6.0)],lo:0.45,hi:0.62,col:"#6aa84f",op:0.95}),rough:(0.6,0.95),metal:0.0,metal_hi:false,nrm:3.0,ao:true},
 m!{name:"lava_rock",base:"noise_voronoi/voronoi",bp:&[("scale",7.0),("style",0.0)],detail:Some(("noise_clouds_2/clouds_2",&[("scale",3.0)],3,0.4)),blur:2.0,steps:0,lv:(0.05,0.8),inv:false,cols:&[(0.0,"#ff7a1a"),(0.18,"#ffcf3a"),(0.22,"#2b1d1d"),(0.6,"#3b2a2a"),(1.0,"#5a4545")],acc:None,rough:(0.5,0.9),metal:0.0,metal_hi:false,nrm:2.5,ao:true},
 m!{name:"sandstone",base:"noise_fractal_sum_base/fractal_sum_base",bp:&[],detail:Some(("noise_directional_noise_1/directional_noise_1",&[],3,0.6)),blur:1.0,steps:6,lv:(0.25,0.75),inv:false,cols:&[(0.0,"#b8814e"),(0.5,"#d9a566"),(1.0,"#f0cc8c")],acc:None,rough:(0.8,1.0),metal:0.0,metal_hi:false,nrm:1.5,ao:false},
 m!{name:"castle_wall",base:"pattern_tile_random_2/tile_random_2",bp:&[("amount_x",5.0),("amount_y",8.0),("interstice_thickness",0.08),("random_size_multiplier",0.5),("round_corners_radius",0.2)],detail:Some(("noise_grunge_map_005/grunge_map_005",&[],3,0.4)),blur:2.0,steps:0,lv:(0.1,0.9),inv:false,cols:&[(0.0,"#2d2b34"),(0.4,"#6a6877"),(1.0,"#a8a6b5")],acc:None,rough:(0.7,0.95),metal:0.0,metal_hi:false,nrm:2.5,ao:true},
 m!{name:"marble",base:"noise_perlin_noise/perlin_noise",bp:&[("scale",3.0)],detail:Some(("noise_plasma/plasma",&[],1,0.25)),blur:0.0,steps:0,lv:(0.3,0.7),inv:false,cols:&[(0.0,"#e8e8f0"),(0.45,"#f7f7fb"),(0.5,"#8a8fa8"),(0.55,"#f7f7fb"),(1.0,"#ffffff")],acc:None,rough:(0.15,0.35),metal:0.0,metal_hi:false,nrm:0.4,ao:false},
 m!{name:"grass",base:"noise_cells_1/cells_1",bp:&[("scale",64.0),("disorder",1.0)],detail:Some(("noise_directional_noise_1/directional_noise_1",&[],1,0.4)),blur:0.0,steps:5,lv:(0.1,0.9),inv:false,cols:&[(0.0,"#2e6b2e"),(0.5,"#52a838"),(1.0,"#a6dd5a")],acc:None,rough:(0.7,0.9),metal:0.0,metal_hi:false,nrm:1.5,ao:true},
 m!{name:"dirt_path",base:"noise_cells_4/cells_4",bp:&[("scale",14.0),("disorder",1.0)],detail:Some(("noise_clouds_2/clouds_2",&[("scale",5.0)],3,0.5)),blur:2.0,steps:5,lv:(0.0,1.0),inv:false,cols:&[(0.0,"#3d2b1f"),(0.5,"#6f4e37"),(1.0,"#a37b55")],acc:Some(Acc{k:"noise_gaussian_spots_1/gaussian_spots_1",p:&[],lo:0.6,hi:0.9,col:"#8d8d95",op:0.9}),rough:(0.85,1.0),metal:0.0,metal_hi:false,nrm:2.0,ao:true},
 m!{name:"sand",base:"noise_perlin_noise/perlin_noise",bp:&[("scale",10.0)],detail:Some(("noise_directional_noise_1/directional_noise_1",&[],1,0.35)),blur:1.0,steps:6,lv:(0.2,0.8),inv:false,cols:&[(0.0,"#d9b878"),(0.5,"#ecd18f"),(1.0,"#fbeab0")],acc:None,rough:(0.85,1.0),metal:0.0,metal_hi:false,nrm:1.2,ao:false},
 m!{name:"mud",base:"noise_clouds_2/clouds_2",bp:&[("scale",3.0)],detail:Some(("noise_gaussian_spots_1/gaussian_spots_1",&[],5,0.5)),blur:2.0,steps:4,lv:(0.2,0.8),inv:false,cols:&[(0.0,"#2a1c12"),(0.5,"#4a3220"),(1.0,"#6b4a30")],acc:None,rough:(0.2,0.6),metal:0.0,metal_hi:false,nrm:2.0,ao:true},
 m!{name:"snow",base:"noise_clouds_2/clouds_2",bp:&[("scale",4.0)],detail:None,blur:1.0,steps:5,lv:(0.2,0.8),inv:false,cols:&[(0.0,"#b7c9e6"),(0.5,"#e0ecfb"),(1.0,"#ffffff")],acc:None,rough:(0.5,0.8),metal:0.0,metal_hi:false,nrm:1.0,ao:false},
 m!{name:"ice",base:"noise_crystal_1/crystal_1",bp:&[("scale",10.0)],detail:Some(("noise_clouds_2/clouds_2",&[],0,0.3)),blur:2.0,steps:0,lv:(0.0,0.8),inv:false,cols:&[(0.0,"#7fc5e6"),(0.5,"#b7e6f7"),(1.0,"#f1fbff")],acc:None,rough:(0.05,0.3),metal:0.0,metal_hi:false,nrm:1.5,ao:false},
 m!{name:"leaves",base:"noise_cells_4/cells_4",bp:&[("scale",10.0),("disorder",0.9)],detail:Some(("noise_perlin_noise/perlin_noise",&[("scale",8.0)],0,0.3)),blur:0.0,steps:0,lv:(0.0,1.0),inv:false,cols:&[(0.0,"#2c5a24"),(0.35,"#4d9a32"),(0.7,"#d9a33a"),(1.0,"#c65a2a")],acc:None,rough:(0.6,0.85),metal:0.0,metal_hi:false,nrm:2.0,ao:true},
 m!{name:"cracked_desert",base:"noise_cells_2/cells_2",bp:&[("scale",7.0),("disorder",0.9)],detail:Some(("noise_clouds_2/clouds_2",&[],3,0.35)),blur:0.5,steps:0,lv:(0.0,0.3),inv:false,cols:&[(0.0,"#3a2212"),(0.12,"#8a5a30"),(0.5,"#d9a566"),(1.0,"#f3d08e")],acc:None,rough:(0.8,1.0),metal:0.0,metal_hi:false,nrm:3.0,ao:true},
 m!{name:"wood_planks",base:"pattern_tile_generator/tile_generator",bp:&[("x_amount",4.0),("y_amount",1.0),("interstice",0.04)],detail:Some(("wood_fibers_1/wood_fibers_1",&[],3,0.8)),blur:1.0,steps:0,lv:(0.0,1.0),inv:false,cols:&[(0.0,"#3a2312"),(0.5,"#8a5a30"),(1.0,"#c58d52")],acc:None,rough:(0.5,0.85),metal:0.0,metal_hi:false,nrm:1.5,ao:true},
 m!{name:"bark",base:"noise_directional_noise_1/directional_noise_1",bp:&[],detail:Some(("noise_cells_1/cells_1",&[("scale",5.0)],3,0.5)),blur:1.0,steps:5,lv:(0.1,0.9),inv:false,cols:&[(0.0,"#2a1d14"),(0.5,"#5a3d28"),(1.0,"#8a6444")],acc:None,rough:(0.8,1.0),metal:0.0,metal_hi:false,nrm:3.0,ao:true},
 m!{name:"log_end",base:"pattern_polygon_1/polygon_1",bp:&[("Sides",32.0),("Gradient",1.0),("Scale",1.0),("AutoScale",1.0)],detail:Some(("noise_clouds_2/clouds_2",&[("scale",2.0)],1,0.08)),blur:0.0,steps:0,lv:(0.0,1.0),inv:false,cols:&[(0.0,"#e0a868"),(0.06,"#8a5a30"),(0.12,"#d09a5c"),(0.18,"#8a5a30"),(0.24,"#d09a5c"),(0.30,"#8a5a30"),(0.36,"#d09a5c"),(0.42,"#8a5a30"),(0.48,"#d09a5c"),(0.54,"#8a5a30"),(0.60,"#d09a5c"),(0.66,"#8a5a30"),(0.72,"#d09a5c"),(0.78,"#8a5a30"),(0.84,"#d09a5c"),(0.90,"#8a5a30"),(1.0,"#3a2312")],acc:None,rough:(0.6,0.9),metal:0.0,metal_hi:false,nrm:1.0,ao:false},
 m!{name:"parquet",base:"pattern_weave_generator/weave_generator",bp:&[("tile_x",3.0),("tile_y",3.0),("shape",0.1),("gap",0.04)],detail:Some(("wood_fibers_2/wood_fibers_2",&[],3,0.7)),blur:1.0,steps:0,lv:(0.0,1.0),inv:false,cols:&[(0.0,"#3b2415"),(0.5,"#9b6a3c"),(1.0,"#d6a064")],acc:None,rough:(0.3,0.6),metal:0.0,metal_hi:false,nrm:1.2,ao:true},
 m!{name:"woven_cloth",base:"pattern_weave_1/weave_1",bp:&[],detail:Some(("noise_gaussian_spots_1/gaussian_spots_1",&[],3,0.2)),blur:0.0,steps:0,lv:(0.0,1.0),inv:false,cols:&[(0.0,"#2a3f6b"),(0.5,"#4a6cb4"),(1.0,"#8fb0f0")],acc:None,rough:(0.8,1.0),metal:0.0,metal_hi:false,nrm:2.0,ao:true},
 m!{name:"leather",base:"noise_cells_1/cells_1",bp:&[("scale",40.0),("disorder",0.7)],detail:Some(("noise_clouds_2/clouds_2",&[],3,0.4)),blur:1.5,steps:0,lv:(0.0,0.9),inv:false,cols:&[(0.0,"#3a1f12"),(0.5,"#7a4526"),(1.0,"#b0703f")],acc:None,rough:(0.4,0.75),metal:0.0,metal_hi:false,nrm:2.5,ao:true},
 m!{name:"wicker",base:"pattern_weave_generator/weave_generator",bp:&[("tile_x",4.0),("tile_y",4.0),("shape",0.5),("weave",3.0),("gap",0.02)],detail:Some(("wood_fibers_1/wood_fibers_1",&[],3,0.5)),blur:0.0,steps:0,lv:(0.0,1.0),inv:false,cols:&[(0.0,"#4a3016"),(0.5,"#b8884a"),(1.0,"#e8c586")],acc:None,rough:(0.7,0.95),metal:0.0,metal_hi:false,nrm:2.5,ao:true},
 m!{name:"rusty_metal",base:"noise_clouds_2/clouds_2",bp:&[("scale",4.0)],detail:Some(("noise_grunge_map_005/grunge_map_005",&[],3,0.6)),blur:0.0,steps:0,lv:(0.2,0.8),inv:false,cols:&[(0.0,"#5a2a14"),(0.5,"#9a4a1e"),(1.0,"#d9873a")],acc:Some(Acc{k:"noise_grunge_map_001/grunge_map_001",p:&[],lo:0.5,hi:0.75,col:"#7a8794",op:0.9}),rough:(0.6,1.0),metal:0.8,metal_hi:true,nrm:1.5,ao:true},
 m!{name:"painted_metal",base:"noise_perlin_noise/perlin_noise",bp:&[("scale",3.0)],detail:Some(("scratches_1/scratches_1",&[],1,0.5)),blur:0.0,steps:0,lv:(0.3,0.7),inv:false,cols:&[(0.0,"#1f6fb5"),(0.6,"#2e8ad8"),(1.0,"#6fb8f0")],acc:Some(Acc{k:"noise_grunge_map_001/grunge_map_001",p:&[],lo:0.75,hi:0.9,col:"#8d949b",op:1.0}),rough:(0.3,0.5),metal:0.6,metal_hi:true,nrm:1.0,ao:false},
 m!{name:"gold",base:"noise_perlin_noise/perlin_noise",bp:&[("scale",5.0)],detail:Some(("scratches_1/scratches_1",&[],1,0.3)),blur:0.0,steps:5,lv:(0.2,0.8),inv:false,cols:&[(0.0,"#a8741a"),(0.5,"#e6b330"),(1.0,"#fff0a0")],acc:None,rough:(0.2,0.4),metal:1.0,metal_hi:false,nrm:0.8,ao:false},
 m!{name:"riveted_steel",base:"pattern_tile_generator/tile_generator",bp:&[("x_amount",3.0),("y_amount",3.0),("interstice",0.03)],detail:Some(("splatter/splatter",&[],1,0.9)),blur:1.0,steps:0,lv:(0.0,1.0),inv:false,cols:&[(0.0,"#2c3238"),(0.5,"#6c7883"),(1.0,"#b8c4ce")],acc:None,rough:(0.3,0.6),metal:1.0,metal_hi:false,nrm:2.5,ao:true},
 m!{name:"metal_grate",base:"pattern_tile_generator/tile_generator",bp:&[("x_amount",10.0),("y_amount",10.0),("interstice",0.35)],detail:None,blur:1.0,steps:0,lv:(0.0,1.0),inv:false,cols:&[(0.0,"#14161a"),(0.45,"#14161a"),(0.5,"#5d6671"),(1.0,"#9aa6b2")],acc:None,rough:(0.4,0.6),metal:1.0,metal_hi:false,nrm:3.0,ao:true},
 m!{name:"hex_tiles",base:"pattern_polygon_1/polygon_1",bp:&[("Sides",6.0),("Tiling",4.0),("Scale",0.9)],detail:Some(("noise_cells_1/cells_1",&[("scale",5.0)],3,0.2)),blur:1.5,steps:0,lv:(0.0,1.0),inv:false,cols:&[(0.0,"#1c2a2f"),(0.5,"#2f8f9d"),(1.0,"#7fe0e8")],acc:None,rough:(0.3,0.6),metal:0.0,metal_hi:false,nrm:2.5,ao:true},
 m!{name:"ceramic_tiles",base:"pattern_tile_generator/tile_generator",bp:&[("x_amount",6.0),("y_amount",6.0),("interstice",0.05)],detail:Some(("noise_clouds_2/clouds_2",&[],3,0.15)),blur:1.5,steps:0,lv:(0.0,1.0),inv:false,cols:&[(0.0,"#5f7f98"),(0.15,"#cfe0ec"),(1.0,"#f4f9fc")],acc:None,rough:(0.1,0.35),metal:0.0,metal_hi:false,nrm:1.5,ao:true},
 m!{name:"plastic",base:"noise_perlin_noise/perlin_noise",bp:&[("scale",2.0)],detail:None,blur:0.0,steps:0,lv:(0.4,0.6),inv:false,cols:&[(0.0,"#d93a4a"),(1.0,"#f0586a")],acc:None,rough:(0.25,0.4),metal:0.0,metal_hi:false,nrm:0.2,ao:false},
 m!{name:"crystal",base:"noise_crystal_2/crystal_2",bp:&[],detail:Some(("noise_cells_1/cells_1",&[("scale",8.0)],3,0.5)),blur:1.0,steps:6,lv:(0.1,0.9),inv:false,cols:&[(0.0,"#5a2d9a"),(0.5,"#b05af0"),(1.0,"#f0b8ff")],acc:None,rough:(0.05,0.3),metal:0.1,metal_hi:false,nrm:2.5,ao:false},
 m!{name:"magic_runes",base:"noise_cells_1/cells_1",bp:&[("scale",6.0)],detail:Some(("pattern_stripes/stripes",&[("Stripes",6.0),("Width",0.2)],5,0.7)),blur:1.0,steps:0,lv:(0.0,1.0),inv:false,cols:&[(0.0,"#1b1d3a"),(0.55,"#2f3470"),(0.6,"#7ad8ff"),(1.0,"#e4f7ff")],acc:None,rough:(0.4,0.8),metal:0.0,metal_hi:false,nrm:2.0,ao:true},
 m!{name:"scifi_panel",base:"pattern_tile_random_2/tile_random_2",bp:&[("amount_x",4.0),("amount_y",4.0),("interstice_thickness",0.05),("random_size_multiplier",0.6)],detail:Some(("pattern_stripes/stripes",&[("Stripes",12.0),("Width",0.1)],1,0.12)),blur:1.0,steps:0,lv:(0.0,1.0),inv:false,cols:&[(0.0,"#0f1418"),(0.3,"#3a4852"),(0.8,"#8fa3b3"),(1.0,"#d8e4ee")],acc:Some(Acc{k:"pattern_stripes/stripes",p:&[("Stripes",2.0),("Width",0.05)],lo:0.2,hi:0.3,col:"#ff7a1a",op:1.0}),rough:(0.3,0.6),metal:0.9,metal_hi:false,nrm:2.5,ao:true},
 m!{name:"slime",base:"noise_cells_2/cells_2",bp:&[("scale",6.0),("disorder",0.5)],detail:Some(("noise_clouds_2/clouds_2",&[],3,0.4)),blur:6.0,steps:0,lv:(0.0,1.0),inv:true,cols:&[(0.0,"#2f8f2f"),(0.5,"#6fe050"),(1.0,"#d2ff8a")],acc:None,rough:(0.02,0.2),metal:0.0,metal_hi:false,nrm:1.5,ao:false},
]}
fn main(){
    let size:i64=std::env::var("SZ").ok().and_then(|v|v.parse().ok()).unwrap_or(1024);
    let only=std::env::args().nth(1);
    std::fs::create_dir_all("/tmp/tpl").unwrap();
    for m in mats(){
        if let Some(o)=&only{ if o!=m.name {continue} }
        let t=std::time::Instant::now();
        let mut s=build(&m,size,&format!("/tmp/tpl/{}.sbs",m.name));
        let o=s.execute("render.outputs",&Json::obj().with("size",256)).unwrap();
        std::fs::write(format!("/tmp/tpl/{}.png",m.name),&o.images[0].png).unwrap();
        let v=run(&mut s,"graph.validate",Json::obj()).to_text();
        println!("{} {:?} {}",m.name,t.elapsed(),v.chars().take(120).collect::<String>().replace('\n'," "));
    }
}
