//! Study tool: decode the IMGD entries of a BAR file (or a bare IMGD) and write each to a PNG,
//! composited over a grey checkerboard so translucent pixels stay visible.
//! usage: dumpimgd IN OUTPREFIX
use std::io::BufWriter;

fn write(path: &str, img: &kh2_image::imgd::Image) {
    let (w, h) = (img.width as usize, img.height as usize);
    let mut rgb = vec![0u8; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let bg = if (x / 8 + y / 8) % 2 == 0 { 96.0 } else { 160.0 };
            let a = img.rgba[i * 4 + 3] as f32 / 255.0;
            for c in 0..3 {
                rgb[i * 3 + c] = (img.rgba[i * 4 + c] as f32 * a + bg * (1.0 - a)) as u8;
            }
        }
    }
    let f = std::fs::File::create(path).unwrap();
    let mut e = png::Encoder::new(BufWriter::new(f), w as u32, h as u32);
    e.set_color(png::ColorType::Rgb);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(&rgb).unwrap();
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&a[1]).unwrap();
    let mut n = 0;
    let imgs: Vec<(String, Vec<u8>)> = match kh2_bar::parse(&data) {
        Ok(es) => es.iter().filter(|e| kh2_image::imgd::is_imgd(e.data)).map(|e| (e.tag.to_string(), e.data.to_vec())).collect(),
        Err(_) => vec![("img".into(), data.clone())],
    };
    for (tag, b) in imgs {
        if let Some(img) = kh2_image::imgd::decode(&b) {
            let p = format!("{}_{}_{}.png", a[2], n, tag);
            write(&p, &img);
            println!("{p}: {}x{} swizzled={}", img.width, img.height, img.swizzled);
            n += 1;
        }
    }
}
