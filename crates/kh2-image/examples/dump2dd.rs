//! Study tool: dump the IMGD/SEQD pairs of a .2dd (images as PNGs; with a third argument `groups`, every animation group
//! with its extents). usage: dump2dd FILE OUTDIR [groups]
use kh2_image::seqd::{Ext, Seqd};
use std::io::BufWriter;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&a[1]).unwrap();
    let out = a.get(2).cloned().unwrap_or("local".into());
    let entries = kh2_bar::parse(&data).unwrap();
    let mut pair = 0;
    let mut i = 0;
    while i + 1 < entries.len() {
        let img = kh2_image::imgd::decode(entries[i].data);
        let seq = kh2_image::seqd::parse(entries[i + 1].data);
        if let (Some(img), Some(seq)) = (img, seq) {
            println!("pair {pair}: name {:?} image {}x{} swz {} groups {} anims {} sprites {} parts {}", entries[i].tag, img.width, img.height, img.swizzled, seq.groups.len(), seq.anims.len(), seq.sprites.len(), seq.parts.len());
            let f = std::fs::File::create(format!("{out}/pair{pair}.png")).unwrap();
            let mut e = png::Encoder::new(BufWriter::new(f), img.width, img.height);
            e.set_color(png::ColorType::Rgba);
            e.set_depth(png::BitDepth::Eight);
            e.write_header().unwrap().write_image_data(&img.rgba).unwrap();
            if a.get(3).map(|s| s == "groups").unwrap_or(false) {
                dump_groups(&seq);
            }
            pair += 1;
            i += 2;
        } else {
            println!("entry {i} {:?} type {} len {}", entries[i].tag, entries[i].kind, entries[i].data.len());
            i += 1;
        }
    }
}

fn dump_groups(seq: &Seqd) {
    for (g, grp) in seq.groups.iter().enumerate() {
        let last = seq.last_frame(g);
        let ext = Ext::default();
        let mut s = String::new();
        for fr in [0, last / 2, last] {
            let qs = seq.quads(g, fr, [0.0, 0.0], &ext);
            let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for q in &qs {
                for p in q.pos {
                    x0 = x0.min(p[0]);
                    x1 = x1.max(p[0]);
                    y0 = y0.min(p[1]);
                    y1 = y1.max(p[1]);
                }
            }
            s += &format!(" f{fr}:[{} q {:.0},{:.0}..{:.0},{:.0}]", qs.len(), x0, y0, x1, y1);
        }
        println!("  g{g}: anims {}..+{} loops {} loop_start {} end {} xy ({},{}) size {} last {}{}", grp.first, grp.count, grp.loops, grp.loop_start, grp.frame_end, grp.x, grp.y, grp.size, last, s);
    }
}
