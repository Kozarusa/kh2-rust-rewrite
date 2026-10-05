//! Study tool: list the texel rectangles each group of a .2dd pair draws (to tell which group shows which
//! picture). usage: sprites FILE PAIR
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&a[1]).unwrap();
    let pair: usize = a[2].parse().unwrap();
    let entries = kh2_bar::parse(&data).unwrap();
    let mut pairs = Vec::new();
    let mut i = 0;
    while i + 1 < entries.len() {
        if let (Some(img), Some(seq)) = (kh2_image::imgd::decode(entries[i].data), kh2_image::seqd::parse(entries[i + 1].data)) {
            pairs.push((img, seq));
            i += 2;
        } else {
            i += 1;
        }
    }
    let seq = &pairs[pair].1;
    for (g, grp) in seq.groups.iter().enumerate() {
        let mut rects = std::collections::BTreeSet::new();
        for an in seq.anims.iter().skip(grp.first).take(grp.count) {
            if let Some(sg) = seq.sprite_groups.get(an.group) {
                for p in seq.parts.iter().skip(sg.start).take(sg.count) {
                    if let Some(sp) = seq.sprites.get(p.sprite) {
                        rects.insert((sp.u0, sp.v0, sp.u1, sp.v1));
                    }
                }
            }
        }
        println!("g{g}: {:?}", rects);
    }
}
