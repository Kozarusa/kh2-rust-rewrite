// scratch (not committed): listing of a magic/*.mag script with extra roots
use kh2_bdx::{Program, analyze_with, listing};

fn main() {
    let path = std::env::var("KH2_ISO").unwrap();
    let mut a = kh2_archive::Archive::open(&path).expect("iso");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let data = a.read(&format!("magic/{}.mag", args[0])).expect("mag");
    let bdx = kh2_bar::parse(&data).unwrap().into_iter().find(|e| e.kind == 3).expect("BDX entry").data.to_vec();
    let prog = Program::parse(&bdx).expect("script");
    let roots: Vec<usize> = args[1..].iter().map(|r| usize::from_str_radix(r, 16).expect("hex root")).collect();
    let an = analyze_with(&prog, &roots).expect("analysis");
    print!("{}", listing(&prog, &an));
}
