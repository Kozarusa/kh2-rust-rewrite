//! Read-only tests against the user's disc (skipped without `KH2_ISO`): `menu/us/save.2ld`, the picture file of the
//! save menu (docs/systems/ui/menus-save.md section 18). The tests print every layer, layout and animation group of the three
//! embedded sequences and check the numbers the doc quotes. `KH2_SAVE_DUMP=<dir>` also rasterises every group at
//! its last frame to `<dir>/s<seq>_g<group>.png` (game art: the directory must lie outside the repository).

use kh2_image::imgd::Image;
use kh2_image::layd;
use kh2_image::seqd::{Ext, Quad, Seqd};

fn open() -> Option<kh2_archive::Archive> {
    let path = kh2_factory::disc_path("menu_save")?;
    kh2_archive::Archive::open(&path).ok()
}

struct Canvas {
    w: i32,
    h: i32,
    ox: f32,
    oy: f32,
    px: Vec<[f32; 4]>,
}

fn draw(cv: &mut Canvas, img: &Image, q: &Quad) {
    for tri in [[0usize, 1, 2], [1, 3, 2]] {
        let p: Vec<[f32; 2]> = tri.iter().map(|&i| [q.pos[i][0] + cv.ox, q.pos[i][1] + cv.oy]).collect();
        let (minx, maxx) = (p.iter().map(|v| v[0]).fold(f32::MAX, f32::min).floor() as i32, p.iter().map(|v| v[0]).fold(f32::MIN, f32::max).ceil() as i32);
        let (miny, maxy) = (p.iter().map(|v| v[1]).fold(f32::MAX, f32::min).floor() as i32, p.iter().map(|v| v[1]).fold(f32::MIN, f32::max).ceil() as i32);
        let den = (p[1][1] - p[2][1]) * (p[0][0] - p[2][0]) + (p[2][0] - p[1][0]) * (p[0][1] - p[2][1]);
        if den.abs() < 1e-6 {
            continue;
        }
        for y in miny.max(0)..=maxy.min(cv.h - 1) {
            for x in minx.max(0)..=maxx.min(cv.w - 1) {
                let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                let l0 = ((p[1][1] - p[2][1]) * (fx - p[2][0]) + (p[2][0] - p[1][0]) * (fy - p[2][1])) / den;
                let l1 = ((p[2][1] - p[0][1]) * (fx - p[2][0]) + (p[0][0] - p[2][0]) * (fy - p[2][1])) / den;
                let l2 = 1.0 - l0 - l1;
                if l0 < 0.0 || l1 < 0.0 || l2 < 0.0 {
                    continue;
                }
                let l = [l0, l1, l2];
                let u: f32 = (0..3).map(|k| q.uv[tri[k]][0] * l[k]).sum();
                let v: f32 = (0..3).map(|k| q.uv[tri[k]][1] * l[k]).sum();
                let (tx, ty) = ((u.floor() as i32).rem_euclid(img.width as i32) as usize, (v.floor() as i32).rem_euclid(img.height as i32) as usize);
                let t = &img.rgba[(ty * img.width as usize + tx) * 4..][..4];
                let mut c = [t[0] as f32 / 255.0, t[1] as f32 / 255.0, t[2] as f32 / 255.0, t[3] as f32 / 255.0];
                for ch in 0..4 {
                    let vc: f32 = (0..3).map(|k| q.color[tri[k]][ch] as f32 * l[k]).sum::<f32>() / 128.0;
                    c[ch] = (c[ch] * vc).min(1.0);
                }
                let d = &mut cv.px[(y * cv.w + x) as usize];
                let a = c[3];
                for i in 0..3 {
                    d[i] = match q.blend {
                        1 => d[i] + c[i] * a,
                        2 => d[i] - c[i] * a,
                        _ => c[i] * a + d[i] * (1.0 - a),
                    }
                    .clamp(0.0, 1.0);
                }
            }
        }
    }
}

fn bbox(qs: &[Quad]) -> [f32; 4] {
    let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for p in qs.iter().flat_map(|q| q.pos) {
        b = [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])];
    }
    b
}

fn write_png(path: &str, w: u32, h: u32, rgba: &[u8]) {
    let f = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    let mut e = png::Encoder::new(f, w, h);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(rgba).unwrap();
}

fn render(seq: &Seqd, img: &Image, group: usize, frame: i32) -> Option<(u32, u32, Vec<u8>)> {
    let qs = seq.quads(group, frame, [0.0, 0.0], &Ext::default());
    if qs.is_empty() {
        return None;
    }
    let b = bbox(&qs);
    let (w, h) = (((b[2] - b[0]).ceil() as i32 + 8).clamp(8, 1500), ((b[3] - b[1]).ceil() as i32 + 8).clamp(8, 1500));
    let mut cv = Canvas { w, h, ox: -b[0] + 4.0, oy: -b[1] + 4.0, px: vec![[0.25, 0.25, 0.3, 1.0]; (w * h) as usize] };
    for q in &qs {
        draw(&mut cv, img, q);
    }
    let rgba: Vec<u8> = cv.px.iter().flat_map(|p| [(p[0] * 255.0) as u8, (p[1] * 255.0) as u8, (p[2] * 255.0) as u8, 255]).collect();
    Some((w as u32, h as u32, rgba))
}

#[test]
fn save_2ld_lists_its_layers_layouts_and_groups() {
    let Some(mut a) = open() else { return };
    let bar = a.read("menu/us/save.2ld").expect("save.2ld");
    let two = layd::parse_2ld(&bar).expect("a .2ld");
    println!("images: {:?}", two.images.iter().map(|i| (i.width, i.height)).collect::<Vec<_>>());
    let l = &two.layd;
    println!("{} layers, layouts {:?}", l.layers.len(), l.layouts);
    for (i, ly) in l.layers.iter().enumerate() {
        println!("  layer {i}: kind {} seq {} group {} at ({}, {})", ly.kind, ly.seq, ly.group, ly.x, ly.y);
    }
    let dump = std::env::var("KH2_SAVE_DUMP").ok();
    for (si, seq) in l.seqs.iter().enumerate() {
        println!("sequence {si}: {} groups, {} anims, {} sprites", seq.groups.len(), seq.anims.len(), seq.sprites.len());
        for (gi, g) in seq.groups.iter().enumerate() {
            let len = seq.group_len(gi);
            let last = (len - 1).max(0);
            let qs = seq.quads(gi, last, [0.0, 0.0], &Ext::default());
            let b = if qs.is_empty() { [0.0; 4] } else { bbox(&qs) };
            println!(
                "  s{si} g{gi:3}: loops {} flags {:#x} loop_start {} end {} len {len} xy ({}, {}) size {} {} {} | last frame: {} quads bbox ({:.0},{:.0})-({:.0},{:.0})",
                g.loops, g.flags, g.loop_start, g.frame_end, g.x, g.y, g.size, g.unk1c, g.unk20, qs.len(), b[0], b[1], b[2], b[3]
            );
            if let Some(dir) = &dump {
                // Sequence k draws from image k when there is one (the three sequences and three images pair up), else image 0.
                let img = two.images.get(si).unwrap_or(&two.images[0]);
                std::fs::create_dir_all(dir).unwrap();
                if let Some((w, h, px)) = render(seq, img, gi, last) {
                    write_png(&format!("{dir}/s{si}_g{gi}.png"), w, h, &px);
                }
            }
        }
    }
    assert_eq!(two.images.len(), 3);
    assert_eq!(l.seqs.iter().map(|s| s.groups.len()).collect::<Vec<_>>(), vec![88, 9, 21]);
}
