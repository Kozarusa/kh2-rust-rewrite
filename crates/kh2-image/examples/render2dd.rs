//! Study tool: software-render sequence groups of a .2dd pair to a PNG on a checker background (HUD units, no
//! y bias). usage: render2dd FILE PAIR OUT.png "group:frame:x:y"... (frame -1 = last, -2 = middle)
use kh2_image::seqd::{Ext, Quad};
use std::io::BufWriter;

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
    let (img, seq) = &pairs[pair];
    let (w, h) = (512usize, 448usize);
    let mut canvas = vec![0f32; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let c = if ((x / 16) + (y / 16)) % 2 == 0 { 0.18 } else { 0.24 };
            canvas[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&[c, c, c]);
        }
    }
    for spec in &a[4..] {
        let p: Vec<f32> = spec.split(':').map(|v| v.parse().unwrap()).collect();
        let g = p[0] as usize; let f = match p[1] as i32 { -1 => seq.last_frame(g), -2 => seq.last_frame(g) / 2, v => v }; let (ox, oy) = (p[2], p[3]);
        let qs = seq.quads(g, f, [ox, oy], &Ext::default());
        for q in &qs {
            draw(&mut canvas, w, h, img, q);
        }
    }
    let mut out = Vec::with_capacity(w * h * 3);
    for v in canvas {
        out.push((v.clamp(0.0, 1.0) * 255.0) as u8);
    }
    let f = std::fs::File::create(&a[3]).unwrap();
    let mut e = png::Encoder::new(BufWriter::new(f), w as u32, h as u32);
    e.set_color(png::ColorType::Rgb);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(&out).unwrap();
}

fn sample(img: &kh2_image::imgd::Image, u: f32, v: f32, nearest: bool) -> [f32; 4] {
    let (iw, ih) = (img.width as i32, img.height as i32);
    let px = |x: i32, y: i32| {
        let (x, y) = (x.rem_euclid(iw) as usize, y.rem_euclid(ih) as usize);
        let i = (y * iw as usize + x) * 4;
        [img.rgba[i] as f32 / 255.0, img.rgba[i + 1] as f32 / 255.0, img.rgba[i + 2] as f32 / 255.0, img.rgba[i + 3] as f32 / 255.0]
    };
    if nearest {
        return px(u.floor() as i32, v.floor() as i32);
    }
    let (u, v) = (u - 0.5, v - 0.5);
    let (x0, y0) = (u.floor(), v.floor());
    let (fx, fy) = (u - x0, v - y0);
    let (x0, y0) = (x0 as i32, y0 as i32);
    let (a, b, c, d) = (px(x0, y0), px(x0 + 1, y0), px(x0, y0 + 1), px(x0 + 1, y0 + 1));
    std::array::from_fn(|k| (a[k] * (1.0 - fx) + b[k] * fx) * (1.0 - fy) + (c[k] * (1.0 - fx) + d[k] * fx) * fy)
}

fn draw(canvas: &mut [f32], w: usize, h: usize, img: &kh2_image::imgd::Image, q: &Quad) {
    // corners TL TR BL BR -> triangles (0,1,2) (1,3,2)
    for tri in [[0usize, 1, 2], [1, 3, 2]] {
        let p: Vec<[f32; 2]> = tri.iter().map(|&i| q.pos[i]).collect();
        let (minx, maxx) = (p.iter().map(|v| v[0]).fold(f32::MAX, f32::min), p.iter().map(|v| v[0]).fold(f32::MIN, f32::max));
        let (miny, maxy) = (p.iter().map(|v| v[1]).fold(f32::MAX, f32::min), p.iter().map(|v| v[1]).fold(f32::MIN, f32::max));
        let area = (p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[1][1] - p[0][1]) * (p[2][0] - p[0][0]);
        if area.abs() < 1e-6 {
            continue;
        }
        for y in (miny.floor().max(0.0) as usize)..(maxy.ceil().min(h as f32) as usize) {
            for x in (minx.floor().max(0.0) as usize)..(maxx.ceil().min(w as f32) as usize) {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let w0 = ((p[1][0] - px) * (p[2][1] - py) - (p[1][1] - py) * (p[2][0] - px)) / area;
                let w1 = ((p[2][0] - px) * (p[0][1] - py) - (p[2][1] - py) * (p[0][0] - px)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let ws = [w0, w1, w2];
                let u: f32 = (0..3).map(|k| ws[k] * q.uv[tri[k]][0]).sum();
                let v: f32 = (0..3).map(|k| ws[k] * q.uv[tri[k]][1]).sum();
                let t = sample(img, u, v, q.nearest);
                let col: [f32; 4] = std::array::from_fn(|c| (0..3).map(|k| ws[k] * q.color[tri[k]][c] as f32 / 128.0).sum());
                let src = [t[0] * col[0], t[1] * col[1], t[2] * col[2]];
                let al = (t[3] * col[3]).min(1.0);
                let dst = &mut canvas[(y * w + x) * 3..(y * w + x) * 3 + 3];
                for c in 0..3 {
                    dst[c] = match q.blend {
                        1 => dst[c] + src[c] * al,
                        2 => dst[c] - src[c] * al,
                        _ => dst[c] * (1.0 - al) + src[c] * al,
                    };
                }
            }
        }
    }
}
