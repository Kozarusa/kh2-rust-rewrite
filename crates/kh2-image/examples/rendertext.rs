//! Study tool: render system messages with the font engine to a PNG (software rasteriser, no GPU).
//! Reads the files `kh2 extract` wrote to local/extract (msg/us/sys.bar, fontinfo.bar, fontimage.bar).
//! usage: rendertext OUT.png FONT SIZE HEXID...
use kh2_image::font::{Font, FontSet};
use kh2_image::text::{layout, shadows, Kind, Messages, Style};
use std::io::BufWriter;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let info = std::fs::read("local/extract/msg/us/fontinfo.bar").unwrap();
    let img = std::fs::read("local/extract/msg/us/fontimage.bar").unwrap();
    let sys = std::fs::read("local/extract/msg/us/sys.bar").unwrap();
    let set = FontSet::parse(&info, &img).unwrap();
    let msgs = Messages::from_bar(&sys).unwrap();
    let font_id: usize = a[2].parse().unwrap();
    let size: f32 = a[3].parse().unwrap();
    let font = Font::get(font_id);
    let (w, h) = (640usize, 480usize);
    let mut canvas = vec![0.25f32; w * h * 3];
    let (aw, ah, arg) = set.atlas_rgba_ps2(font.atlas, font.clut_row());
    let (iw, ih, icn) = set.icons_rgba_ps2();
    let mut y = 8.0;
    for id in &a[4..] {
        let id = u32::from_str_radix(id.trim_start_matches("0x"), 16).unwrap();
        let toks = msgs.tokens(id).unwrap();
        println!("{id:#x}: {:?}", kh2_image::text::to_text(&toks));
        let l = layout(&set, None, &toks, &Style { font: font_id, size: Some(size), ..Style::default() }, [10.0, y]);
        let mut all = shadows(&l.quads, font.mode, 0x80);
        all.extend(l.quads.iter().copied());
        for q in &all {
            let (data, tw, th) = match q.kind {
                Kind::Glyph(_) => (&arg, aw, ah),
                Kind::Icon => (&icn, iw, ih),
            };
            let (x0, y0, x1, y1) = (q.pos[0], q.pos[1], q.pos[2], q.pos[3]);
            for py in (y0.floor() as i32).max(0)..(y1.ceil() as i32).min(h as i32) {
                for px in (x0.floor() as i32).max(0)..(x1.ceil() as i32).min(w as i32) {
                    let u = q.uv[0] + (px as f32 + 0.5 - x0) / (x1 - x0) * (q.uv[2] - q.uv[0]);
                    let v = q.uv[1] + (py as f32 + 0.5 - y0) / (y1 - y0) * (q.uv[3] - q.uv[1]);
                    let (ux, vy) = ((u - 0.5).round().clamp(0.0, tw as f32 - 1.0) as usize, (v - 0.5).round().clamp(0.0, th as f32 - 1.0) as usize);
                    let t = &data[(vy * tw + ux) * 4..(vy * tw + ux) * 4 + 4];
                    let al = t[3] as f32 / 255.0 * q.color[3] as f32 / 128.0;
                    let dst = &mut canvas[(py as usize * w + px as usize) * 3..][..3];
                    for c in 0..3 {
                        let src = t[c] as f32 / 255.0 * q.color[c] as f32 / 128.0;
                        dst[c] = dst[c] * (1.0 - al) + src * al;
                    }
                }
            }
        }
        y += l.size[1] + 8.0;
    }
    let mut out = Vec::new();
    for v in canvas {
        out.push((v.clamp(0.0, 1.0) * 255.0) as u8);
    }
    let f = std::fs::File::create(&a[1]).unwrap();
    let mut e = png::Encoder::new(BufWriter::new(f), w as u32, h as u32);
    e.set_color(png::ColorType::Rgb);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(&out).unwrap();
}
