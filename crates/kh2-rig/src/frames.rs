//! The rig's frame part: what is drawn, original versus port (`kh2-rig frames`, called by `tools/rig.ps1`).
//!
//! # The common frame
//!
//! Both sides are brought to the rig size, 640x448 (the PS2's internal NTSC frame), before they are compared:
//!
//! * The port renders into a window the harness sizes to 640x448 (`KH2_SHOT_SIZE`), so its screenshot is used as it is. A screenshot
//!   of another size goes through the same crop and scale as the original's.
//! * The original is a capture of the PCSX2 window's client area (`tools/pcsx2_shot.py`). The largest centred 4:3 rectangle of it is
//!   cut out (this drops the black bars PCSX2 adds around a 4:3 picture in a window of another shape, and any status bar; the PS2
//!   picture has no overscan border in PCSX2), and that rectangle is scaled to 640x448 with a triangle (bilinear) filter. The
//!   content area is therefore the whole 4:3 picture; if the window is exactly 4:3 nothing is lost, if it is 16:9 the sides go.
//!
//! # The scores
//!
//! Both images are turned to luma (BT.601: 0.299 R + 0.587 G + 0.114 B, 0..255). Pixels inside a shot's `mask` rectangles are left
//! out of both scores.
//!
//! * **SSIM**: the structural similarity index on luma, with 8x8 windows moved by 4 pixels, constants (0.01 x 255)^2 and
//!   (0.03 x 255)^2, uniform window weights. Only the unmasked pixels of a window enter its mean, variance and covariance, and a
//!   window with fewer than half its pixels unmasked is skipped, so a masked region cannot change the score. The score is the mean
//!   over the windows (1 = identical).
//! * **MAD**: the mean absolute luma difference over the unmasked pixels (0 = identical, 255 = black versus white).
//!
//! A shot passes when SSIM >= `tol.ssim` and MAD <= `tol.mad`; the report line names the number that failed. SSIM catches changed
//! structure (edges, geometry, a missing layer) and is blind to a uniform change of brightness or colour; MAD catches those.
//!
//! # Output
//!
//! `<out>/frames/<name>-orig.png`, `-port.png` (both at 640x448), `-diff.png` (the absolute luma difference, four times amplified, masked
//! pixels dark blue) and `-side.png` (original, port and diff next to each other, 1920x448); `frames.csv` and `frames.txt` in `<out>`.

use crate::scenario::{Rect, Scenario, Shot};
use image::{Rgb, RgbImage, imageops};
use std::path::{Path, PathBuf};

pub const RIG_W: u32 = 640;
pub const RIG_H: u32 = 448;

const WIN: usize = 8;
const STRIDE: usize = 4;
const C1: f64 = (0.01 * 255.0) * (0.01 * 255.0);
const C2: f64 = (0.03 * 255.0) * (0.03 * 255.0);

/// The largest centred 4:3 rectangle of a `w` x `h` image: `(x, y, w, h)`.
pub fn content_rect(w: u32, h: u32) -> (u32, u32, u32, u32) {
    // 4:3 means cw * 3 == ch * 4.
    if u64::from(w) * 3 >= u64::from(h) * 4 {
        let (cw, ch) = ((h / 3) * 4, (h / 3) * 3);
        ((w - cw) / 2, (h - ch) / 2, cw, ch)
    } else {
        let (cw, ch) = ((w / 4) * 4, (w / 4) * 3);
        ((w - cw) / 2, (h - ch) / 2, cw, ch)
    }
}

/// Brings a capture of any size to the 640x448 rig frame (see the module notes).
pub fn fit_to_rig(img: &RgbImage) -> RgbImage {
    if img.dimensions() == (RIG_W, RIG_H) {
        return img.clone();
    }
    let (x, y, w, h) = content_rect(img.width(), img.height());
    let cropped = imageops::crop_imm(img, x, y, w.max(1), h.max(1)).to_image();
    imageops::resize(&cropped, RIG_W, RIG_H, imageops::FilterType::Triangle)
}

/// Luma (0..255) of every pixel, row by row.
pub fn luma(img: &RgbImage) -> Vec<f32> {
    img.pixels().map(|p| 0.299 * f32::from(p[0]) + 0.587 * f32::from(p[1]) + 0.114 * f32::from(p[2])).collect()
}

/// `true` for every pixel that counts (outside all mask rectangles).
pub fn valid_pixels(w: u32, h: u32, mask: &[Rect]) -> Vec<bool> {
    let mut v = vec![true; (w * h) as usize];
    for r in mask {
        for y in r[1]..(r[1] + r[3]).min(h) {
            for x in r[0]..(r[0] + r[2]).min(w) {
                v[(y * w + x) as usize] = false;
            }
        }
    }
    v
}

/// SSIM of two luma planes of `w` x `h` over the pixels where `valid` is true; `None` when no window has enough valid pixels.
pub fn ssim(a: &[f32], b: &[f32], w: usize, h: usize, valid: &[bool]) -> Option<f64> {
    assert!(a.len() == w * h && b.len() == w * h && valid.len() == w * h, "ssim: plane sizes differ");
    let (mut sum, mut n) = (0.0f64, 0u32);
    let mut y = 0;
    while y + WIN <= h {
        let mut x = 0;
        while x + WIN <= w {
            let (mut cnt, mut sa, mut sb, mut saa, mut sbb, mut sab) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
            for yy in y..y + WIN {
                for xx in x..x + WIN {
                    let i = yy * w + xx;
                    if valid[i] {
                        let (p, q) = (f64::from(a[i]), f64::from(b[i]));
                        cnt += 1.0;
                        sa += p;
                        sb += q;
                        saa += p * p;
                        sbb += q * q;
                        sab += p * q;
                    }
                }
            }
            if cnt * 2.0 >= (WIN * WIN) as f64 {
                let (ma, mb) = (sa / cnt, sb / cnt);
                let (va, vb, cov) = (saa / cnt - ma * ma, sbb / cnt - mb * mb, sab / cnt - ma * mb);
                sum += ((2.0 * ma * mb + C1) * (2.0 * cov + C2)) / ((ma * ma + mb * mb + C1) * (va + vb + C2));
                n += 1;
            }
            x += STRIDE;
        }
        y += STRIDE;
    }
    (n > 0).then(|| sum / f64::from(n))
}

/// Mean absolute difference over the valid pixels; `None` when none is valid.
pub fn mad(a: &[f32], b: &[f32], valid: &[bool]) -> Option<f64> {
    let (mut sum, mut n) = (0.0f64, 0u64);
    for i in 0..a.len() {
        if valid[i] {
            sum += f64::from((a[i] - b[i]).abs());
            n += 1;
        }
    }
    (n > 0).then(|| sum / n as f64)
}

/// The scores of one shot and what they were judged against.
#[derive(Clone, Debug, PartialEq)]
pub struct ShotResult {
    pub name: String,
    pub k: u32,
    pub ssim: Option<f64>,
    pub mad: Option<f64>,
    pub ssim_min: f64,
    pub mad_max: f64,
    /// A capture was missing or unreadable (the text says which); the shot fails.
    pub error: Option<String>,
}

impl ShotResult {
    pub fn ssim_ok(&self) -> bool {
        self.ssim.is_some_and(|s| s >= self.ssim_min)
    }

    pub fn mad_ok(&self) -> bool {
        self.mad.is_some_and(|m| m <= self.mad_max)
    }

    pub fn passed(&self) -> bool {
        self.error.is_none() && self.ssim_ok() && self.mad_ok()
    }

    /// `FRAME <name> k <k>: ssim 0.9123 (min 0.90) mad 4.21 (max 12.00) PASS|FAIL [ssim] [mad]`
    pub fn line(&self) -> String {
        if let Some(e) = &self.error {
            return format!("FRAME {} k {}: FAIL ({e})", self.name, self.k);
        }
        let num = |v: Option<f64>| v.map_or("n/a".to_string(), |v| format!("{v:.4}"));
        let mut s = format!("FRAME {} k {}: ssim {} (min {}) mad {} (max {})", self.name, self.k, num(self.ssim), self.ssim_min, num(self.mad), self.mad_max);
        if self.passed() {
            s.push_str(" PASS");
        } else {
            s.push_str(" FAIL");
            if !self.ssim_ok() {
                s.push_str(" [ssim]");
            }
            if !self.mad_ok() {
                s.push_str(" [mad]");
            }
        }
        s
    }
}

/// The images of a comparison: both frames at rig size and the diff picture.
pub struct Compared {
    pub orig: RgbImage,
    pub port: RgbImage,
    pub diff: RgbImage,
    pub side: RgbImage,
    pub result: ShotResult,
}

/// Compares two captures (any size, fitted first) under a shot's mask and tolerances.
pub fn compare_images(shot: &Shot, orig: &RgbImage, port: &RgbImage) -> Compared {
    let (orig, port) = (fit_to_rig(orig), fit_to_rig(port));
    let (la, lb) = (luma(&orig), luma(&port));
    let valid = valid_pixels(RIG_W, RIG_H, &shot.mask);
    let result = ShotResult {
        name: shot.name.clone(),
        k: shot.at,
        ssim: ssim(&la, &lb, RIG_W as usize, RIG_H as usize, &valid),
        mad: mad(&la, &lb, &valid),
        ssim_min: shot.ssim_min,
        mad_max: shot.mad_max,
        error: None,
    };
    let mut diff = RgbImage::new(RIG_W, RIG_H);
    for (i, p) in diff.pixels_mut().enumerate() {
        *p = if valid[i] {
            let d = ((la[i] - lb[i]).abs() * 4.0).min(255.0) as u8;
            Rgb([d, d, d])
        } else {
            Rgb([0, 0, 90])
        };
    }
    let mut side = RgbImage::new(RIG_W * 3, RIG_H);
    for (n, img) in [&orig, &port, &diff].into_iter().enumerate() {
        imageops::replace(&mut side, img, i64::from(RIG_W) * n as i64, 0);
    }
    Compared { orig, port, diff, side, result }
}

/// Result of the frame part of a scenario.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrameReport {
    pub scenario: String,
    pub shots: Vec<ShotResult>,
}

impl FrameReport {
    pub fn passed(&self) -> bool {
        !self.shots.is_empty() && self.shots.iter().all(ShotResult::passed)
    }

    pub fn text(&self) -> String {
        let mut s = format!("frame scenario {}\n\n", self.scenario);
        for r in &self.shots {
            s.push_str(&r.line());
            s.push('\n');
        }
        let p = self.shots.iter().filter(|r| r.passed()).count();
        s.push_str(&format!("\nFRAMES {} {} {p}/{}\n", self.scenario, if self.passed() { "PASS" } else { "FAIL" }, self.shots.len()));
        s
    }

    /// `scenario,name,k,ssim,ssim_min,mad,mad_max,status` rows.
    pub fn csv(&self) -> String {
        let mut s = String::from("scenario,name,k,ssim,ssim_min,mad,mad_max,status\n");
        for r in &self.shots {
            let n = |v: Option<f64>| v.map_or(String::new(), |v| v.to_string());
            s.push_str(&format!("{},{},{},{},{},{},{},{}\n", self.scenario, r.name, r.k, n(r.ssim), r.ssim_min, n(r.mad), r.mad_max, if r.passed() { "PASS" } else { "FAIL" }));
        }
        s
    }
}

/// File name of the port's screenshot at tick `k` (`KH2_SHOT_TICKS`) and of the original's raw window capture, in the run folder.
pub fn port_shot_name(k: u32) -> String {
    format!("shot-{k}.png")
}

pub fn orig_shot_name(name: &str) -> String {
    format!("shot-orig-{name}.png")
}

fn load(path: &Path) -> Result<RgbImage, String> {
    image::open(path).map(|i| i.to_rgb8()).map_err(|e| format!("{}: {e}", path.display()))
}

/// Compares every `[[shot]]` of `sc` from the captures in `dir` (`shot-orig-<name>.png`, `shot-<k>.png`) and writes the pictures into
/// `dir/frames/`. The shots keep their order in the scenario file.
pub fn compare_dir(sc: &Scenario, dir: &Path) -> Result<FrameReport, String> {
    let out = dir.join("frames");
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut report = FrameReport { scenario: sc.name.clone(), shots: Vec::new() };
    for shot in &sc.shots {
        let (po, pp): (PathBuf, PathBuf) = (dir.join(orig_shot_name(&shot.name)), dir.join(port_shot_name(shot.at)));
        let fail = |e: String| ShotResult { name: shot.name.clone(), k: shot.at, ssim: None, mad: None, ssim_min: shot.ssim_min, mad_max: shot.mad_max, error: Some(e) };
        let (o, p) = match (load(&po), load(&pp)) {
            (Ok(o), Ok(p)) => (o, p),
            (Err(e), _) | (_, Err(e)) => {
                report.shots.push(fail(e));
                continue;
            }
        };
        let c = compare_images(shot, &o, &p);
        for (suffix, img) in [("orig", &c.orig), ("port", &c.port), ("diff", &c.diff), ("side", &c.side)] {
            let f = out.join(format!("{}-{suffix}.png", shot.name));
            img.save(&f).map_err(|e| format!("{}: {e}", f.display()))?;
        }
        report.shots.push(c.result);
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A deterministic test picture: a smooth gradient with a hard-edged box, so every window has structure.
    fn picture() -> RgbImage {
        RgbImage::from_fn(RIG_W, RIG_H, |x, y| {
            let inside = (200..300).contains(&x) && (100..200).contains(&y);
            let v = if inside { 240 } else { ((x / 3 + y / 2) % 200) as u8 };
            Rgb([v, v / 2 + 20, 255 - v])
        })
    }

    fn noise(seed: u32) -> RgbImage {
        let mut s = seed;
        RgbImage::from_fn(RIG_W, RIG_H, |_, _| {
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let v = (s >> 24) as u8;
            Rgb([v, v, v])
        })
    }

    fn shot(mask: Vec<Rect>) -> Shot {
        Shot { at: 5, name: "t".into(), mask, ssim_min: 0.9, mad_max: 12.0 }
    }

    fn scores(a: &RgbImage, b: &RgbImage, mask: &[Rect]) -> (f64, f64) {
        let c = compare_images(&shot(mask.to_vec()), a, b);
        (c.result.ssim.unwrap(), c.result.mad.unwrap())
    }

    #[test]
    fn identical_images_score_one_and_zero() {
        let p = picture();
        let (s, m) = scores(&p, &p, &[]);
        assert!((s - 1.0).abs() < 1e-12, "ssim {s}");
        assert_eq!(m, 0.0);
        assert!(compare_images(&shot(vec![]), &p, &p).result.passed());
    }

    #[test]
    fn a_changed_region_that_is_masked_does_not_count() {
        let a = picture();
        let mut b = a.clone();
        for y in 300..360 {
            for x in 40..140 {
                b.put_pixel(x, y, Rgb([255, 0, 0]));
            }
        }
        let (s_unmasked, m_unmasked) = scores(&a, &b, &[]);
        assert!(s_unmasked < 0.999 && m_unmasked > 0.5, "the change is visible without a mask: ssim {s_unmasked} mad {m_unmasked}");
        let (s, m) = scores(&a, &b, &[[40, 300, 100, 60]]);
        assert!((s - 1.0).abs() < 1e-12, "masked: ssim {s}");
        assert_eq!(m, 0.0);
        // A mask that misses part of the change leaves the rest in.
        let (s2, _) = scores(&a, &b, &[[40, 300, 50, 60]]);
        assert!(s2 < 1.0);
    }

    #[test]
    fn noise_against_a_picture_scores_low_and_fails_both_numbers() {
        let c = compare_images(&shot(vec![]), &picture(), &noise(7));
        assert!(c.result.ssim.unwrap() < 0.2, "ssim {:?}", c.result.ssim);
        assert!(c.result.mad.unwrap() > 40.0, "mad {:?}", c.result.mad);
        let line = c.result.line();
        assert!(line.contains("FAIL [ssim] [mad]"), "{line}");
        // Two different noise images are unrelated too.
        assert!(scores(&noise(1), &noise(2), &[]).0 < 0.1);
    }

    #[test]
    fn a_uniform_brightness_shift_fails_mad_but_not_ssim() {
        let a = picture();
        let b = RgbImage::from_fn(RIG_W, RIG_H, |x, y| {
            let p = a.get_pixel(x, y);
            Rgb([p[0].saturating_add(30), p[1].saturating_add(30), p[2].saturating_add(30)])
        });
        let c = compare_images(&shot(vec![]), &a, &b);
        assert!(c.result.ssim.unwrap() > 0.9, "ssim {:?}", c.result.ssim);
        assert!(!c.result.mad_ok() && c.result.ssim_ok());
        let line = c.result.line();
        assert!(line.ends_with("FAIL [mad]"), "{line}");
    }

    #[test]
    fn everything_masked_gives_no_score_and_fails() {
        let p = picture();
        let c = compare_images(&shot(vec![[0, 0, 640, 448]]), &p, &p);
        assert_eq!((c.result.ssim, c.result.mad), (None, None));
        assert!(!c.result.passed());
        assert!(c.result.line().contains("FAIL [ssim] [mad]"));
    }

    #[test]
    fn content_rect_is_the_largest_centred_four_by_three() {
        assert_eq!(content_rect(640, 480), (0, 0, 640, 480));
        assert_eq!(content_rect(1280, 720), (160, 0, 960, 720), "16:9 loses its sides");
        assert_eq!(content_rect(1024, 1024), (0, 128, 1024, 768), "a square loses top and bottom");
        assert_eq!(content_rect(800, 600), (0, 0, 800, 600));
        for (w, h) in [(1920, 1080), (1000, 700), (333, 777), (4, 3)] {
            let (x, y, cw, ch) = content_rect(w, h);
            assert_eq!(cw * 3, ch * 4, "{w}x{h}: {cw}x{ch} is 4:3");
            assert!(x + cw <= w && y + ch <= h);
            assert!(x <= w - x - cw + 1 && y <= h - y - ch + 1, "centred");
        }
    }

    #[test]
    fn fit_crops_black_bars_and_scales_to_the_rig_frame() {
        // A 1280x720 window with the 4:3 picture in the middle (pillarbox): the picture, scaled, comes out.
        let base = picture();
        let scaled = imageops::resize(&base, 960, 720, imageops::FilterType::Triangle);
        let mut window = RgbImage::new(1280, 720);
        imageops::replace(&mut window, &scaled, 160, 0);
        let fitted = fit_to_rig(&window);
        assert_eq!(fitted.dimensions(), (RIG_W, RIG_H));
        let (s, m) = scores(&base, &fitted, &[]);
        assert!(s > 0.8 && m < 6.0, "round trip through a pillarboxed window: ssim {s} mad {m}");
        // Already at rig size: unchanged.
        assert_eq!(fit_to_rig(&base), base);
    }

    #[test]
    fn the_diff_picture_marks_masked_pixels_and_amplifies_differences() {
        let a = picture();
        let mut b = a.clone();
        b.put_pixel(10, 10, Rgb([0, 0, 0]));
        let c = compare_images(&shot(vec![[100, 100, 10, 10]]), &a, &b);
        assert_eq!(c.side.dimensions(), (RIG_W * 3, RIG_H));
        assert_eq!(*c.diff.get_pixel(105, 105), Rgb([0, 0, 90]));
        let d = *c.diff.get_pixel(10, 10);
        assert!(d[0] > 0 && d[0] == d[1] && d[1] == d[2]);
        assert_eq!(*c.diff.get_pixel(11, 11), Rgb([0, 0, 0]));
    }

    #[test]
    fn a_directory_of_captures_is_compared_and_missing_files_fail() {
        let dir = std::env::temp_dir().join(format!("kh2-rig-frames-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sc = Scenario::parse("[scenario]\nname = \"f\"\nstate = \"06\"\nframes = 10\n[[shot]]\nat = 5\nname = \"a\"\n[[shot]]\nat = 8\nname = \"b\"\n", Path::new(".")).unwrap();
        picture().save(dir.join(orig_shot_name("a"))).unwrap();
        picture().save(dir.join(port_shot_name(5))).unwrap();
        let r = compare_dir(&sc, &dir).unwrap();
        assert_eq!(r.shots.len(), 2);
        assert!(r.shots[0].passed(), "{}", r.shots[0].line());
        assert!(r.shots[1].error.as_deref().unwrap().contains("shot-orig-b.png"));
        assert!(!r.passed());
        for f in ["a-orig.png", "a-port.png", "a-diff.png", "a-side.png"] {
            assert!(dir.join("frames").join(f).is_file(), "{f}");
        }
        let text = r.text();
        assert!(text.contains("FRAME a k 5: ssim 1.0000 (min 0.9) mad 0.0000 (max 12) PASS"), "{text}");
        assert!(text.ends_with("FRAMES f FAIL 1/2\n"), "{text}");
        assert!(r.csv().starts_with("scenario,name,k,ssim,ssim_min,mad,mad_max,status\nf,a,5,1,0.9,0,12,PASS\n"), "{}", r.csv());
        std::fs::remove_dir_all(&dir).ok();
    }
}
