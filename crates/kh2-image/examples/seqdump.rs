//! Developer tool: rasterises sequence animation groups from a `.2dd` BAR on disk to PNG files,
//! for checking the decoders. Output is game art: write it outside the repository.
//!
//! usage: seqdump FILE IMAGE_ENTRY OUT_DIR [GROUP[:FRAME]]... | summary | anims GROUP...
//! IMAGE_ENTRY is the BAR index of an IMGD entry; the SEQD must be the entry after it. With no
//! groups it lists every group.

use std::fs::File;
use std::io::BufWriter;

use kh2_image::imgd;
use kh2_image::seqd::{self, Ext, Quad};

struct Canvas {
    w: i32,
    h: i32,
    ox: f32,
    oy: f32,
    px: Vec<[f32; 4]>,
}

impl Canvas {
    fn put(&mut self, x: i32, y: i32, c: [f32; 4], blend: u32) {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return;
        }
        let d = &mut self.px[(y * self.w + x) as usize];
        let a = c[3];
        for i in 0..3 {
            d[i] = match blend {
                1 => d[i] + c[i] * a,
                2 => d[i] - c[i] * a,
                _ => c[i] * a + d[i] * (1.0 - a),
            }
            .clamp(0.0, 1.0);
        }
        d[3] = 1.0;
    }
}

fn draw(cv: &mut Canvas, img: &imgd::Image, q: &Quad) {
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
                cv.put(x, y, c, q.blend);
            }
        }
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&a[1]).expect("read");
    let pair: usize = a[2].parse().unwrap();
    let ents = kh2_bar::parse(&data).expect("bar");
    let img = imgd::decode(ents[pair].data).expect("imgd");
    let seq = seqd::parse(ents[pair + 1].data).expect("seqd");
    println!("image {}x{} swizzled={} sprites={} parts={} sgroups={} anims={} groups={}", img.width, img.height, img.swizzled, seq.sprites.len(), seq.parts.len(), seq.sprite_groups.len(), seq.anims.len(), seq.groups.len());
    let out = &a[3];
    {
        let f = BufWriter::new(File::create(format!("{out}/atlas.png")).unwrap());
        let mut e = png::Encoder::new(f, img.width, img.height);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().unwrap().write_image_data(&img.rgba).unwrap();
    }
    if a.len() == 5 && a[4] == "sprites" {
        for (i, s) in seq.sprites.iter().enumerate() {
            println!("sprite {i}: pal {} uv ({},{})-({},{}) scroll ({}, {}) colors {:?}", s.palette, s.u0, s.v0, s.u1, s.v1, s.u_scroll, s.v_scroll, s.colors);
        }
        return;
    }
    if a.len() >= 6 && a[4] == "anims" {
        for spec in &a[5..] {
            let g: usize = spec.parse().unwrap();
            let gr = seq.groups[g];
            println!("group {g}: loops {} flags {:#x} loop_start {} frame_end {} len {}", gr.loops, gr.flags, gr.loop_start, gr.frame_end, seq.group_len(g));
            for an in seq.anims.iter().skip(gr.first).take(gr.count) {
                println!(
                    "  frames {}..{} flags {:#x} sg {} tx {:?} ty {:?} pivot {:?} {:?} rotz {:?} scale {:?} sx {:?} sy {:?} bounce {:?} {:?} blend {} col {:?}->{:?} curve {} {}",
                    an.frame_start, an.frame_end, an.flags, an.group, an.translate_x, an.translate_y, an.pivot_x, an.pivot_y, an.rot_z, an.scale, an.scale_x, an.scale_y, an.bounce_x, an.bounce_y, an.blend, an.color_start, an.color_end, an.curve_start, an.curve_end
                );
            }
        }
        return;
    }
    if a.len() == 5 && a[4] == "summary" {
        // One line per group: quads at frame 0 and the middle frame, bounding box, average texel colour.
        for i in 0..seq.groups.len() {
            let mid = seq.last_frame(i) / 2;
            for f in [0, mid] {
                let qs = seq.quads(i, f, [0.0, 0.0], &Ext::default());
                if qs.is_empty() {
                    continue;
                }
                let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
                let (mut sum, mut n) = ([0f32; 3], 0f32);
                for q in &qs {
                    for p in q.pos {
                        x0 = x0.min(p[0]);
                        x1 = x1.max(p[0]);
                        y0 = y0.min(p[1]);
                        y1 = y1.max(p[1]);
                    }
                    let (u0, v0, u1, v1) = (q.uv[0][0] as usize, q.uv[0][1] as usize, q.uv[3][0] as usize, q.uv[3][1] as usize);
                    for y in v0..v1.min(img.height as usize) {
                        for x in u0..u1.min(img.width as usize) {
                            let t = &img.rgba[(y * img.width as usize + x) * 4..][..4];
                            if t[3] > 128 {
                                for c in 0..3 {
                                    sum[c] += t[c] as f32;
                                }
                                n += 1.0;
                            }
                        }
                    }
                }
                let avg = if n > 0.0 { [sum[0] / n, sum[1] / n, sum[2] / n] } else { [0.0; 3] };
                println!("g{i:3} f{f:3} quads {:2} bbox ({x0:5.0},{y0:5.0})-({x1:5.0},{y1:5.0}) avg rgb ({:3.0},{:3.0},{:3.0})", qs.len(), avg[0], avg[1], avg[2]);
                if f == mid {
                    break;
                }
            }
        }
        return;
    }
    if a.len() == 4 {
        for (i, g) in seq.groups.iter().enumerate() {
            let last = seq.last_frame(i);
            println!("group {i}: anims {}..+{} loops {} flags {:#x} loop_start {} end {} len {} xy ({}, {}) size {} {} {}", g.first, g.count, g.loops, g.flags, g.loop_start, g.frame_end, seq.group_len(i), g.x, g.y, g.size, g.unk1c, g.unk20);
            let _ = last;
        }
        return;
    }
    for spec in &a[4..] {
        let (g, f) = match spec.split_once(':') {
            Some((g, f)) => (g.parse::<usize>().unwrap(), f.parse::<i32>().unwrap()),
            None => (spec.parse::<usize>().unwrap(), 0),
        };
        let quads = seq.quads(g, f, [0.0, 0.0], &Ext::default());
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for q in &quads {
            for p in q.pos {
                x0 = x0.min(p[0]);
                x1 = x1.max(p[0]);
                y0 = y0.min(p[1]);
                y1 = y1.max(p[1]);
            }
        }
        if quads.is_empty() {
            println!("group {g} frame {f}: nothing drawn");
            continue;
        }
        let (w, h) = (((x1 - x0).ceil() as i32 + 8).clamp(8, 1500), ((y1 - y0).ceil() as i32 + 8).clamp(8, 1500));
        // Dark grey backdrop so black and white pieces are visible.
        let mut cv = Canvas { w, h, ox: -x0 + 4.0, oy: -y0 + 4.0, px: vec![[0.25, 0.25, 0.3, 1.0]; (w * h) as usize] };
        for q in &quads {
            draw(&mut cv, &img, q);
        }
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for p in &cv.px {
            rgba.extend_from_slice(&[(p[0] * 255.0) as u8, (p[1] * 255.0) as u8, (p[2] * 255.0) as u8, 255]);
        }
        let file = BufWriter::new(File::create(format!("{out}/g{g}_f{f}.png")).unwrap());
        let mut e = png::Encoder::new(file, w as u32, h as u32);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().unwrap().write_image_data(&rgba).unwrap();
        println!("group {g} frame {f}: {} quads, bbox ({x0:.0},{y0:.0})-({x1:.0},{y1:.0})", quads.len());
    }
}
