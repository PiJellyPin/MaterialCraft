use materialcraft::json::Json;
use materialcraft::session::Session;
fn run(s:&mut Session,c:&str,p:Json)->Json{ match s.execute(c,&p){Ok(o)=>o.data,Err(e)=>panic!("{c}: {e}")} }
fn main(){
    let mut s=Session::new();
    run(&mut s,"graph.new",Json::obj().with("template","material").with("size",512).with("name","g"));
    println!("{}",run(&mut s,"spline.set",Json::obj().with("preset","star").with("name","star1")).to_text());
    run(&mut s,"node.add",Json::obj().with("kind","sbs:lib:bevel/bevel").with("name","bev").with("inputs",Json::obj().with("input",Json::from(13))));
    let o=s.execute("render.node",&Json::obj().with("node","bev").with("size",256)).unwrap();
    std::fs::write("/tmp/spl_bevel.png",&o.images[0].png).unwrap();
    let o=s.execute("render.node",&Json::obj().with("node",13).with("size",256)).unwrap();
    std::fs::write("/tmp/spl_raw.png",&o.images[0].png).unwrap();
    run(&mut s,"graph.save",Json::obj().with("path","/tmp/spl.sbs"));
    let mut s2=Session::new();
    run(&mut s2,"graph.open",Json::obj().with("path","/tmp/spl.sbs"));
    println!("{}",run(&mut s2,"resource.list",Json::obj()).to_text());
    println!("{}",run(&mut s2,"spline.get",Json::obj().with("node",13)).to_text().chars().take(300).collect::<String>());
    println!("{}",run(&mut s2,"graph.validate",Json::obj()).to_text());
}
