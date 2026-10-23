//! A small writer for the COCT format `Coct::parse` and `oct::Oct` read, for tests and tools that need a
//! collision file without the disc (the layout is the one `FUN_00141500` builds its object from, see
//! `docs/systems/core-runtime/collision.md` sections 8 and 9.2).
//!
//! The file has one tree node over everything, one mesh per distinct group, and every polygon without its own
//! box. Coordinates are the game's (Y down, the form the file stores); level 0 (unscaled boxes).

/// One polygon to write.
#[derive(Clone, Debug)]
pub struct BuildPoly {
    /// Three or four vertices in the game's space; the normal follows the winding (`(v1 - v0) x (v2 - v0)`).
    pub verts: Vec<[f32; 3]>,
    /// The attribute word (`flags::PARTY_COLLIDE` and so on, low nibble the floor kind).
    pub attr: u32,
    /// The mesh group (a bit number of the map's visibility word).
    pub group: u8,
}

impl BuildPoly {
    pub fn new(verts: &[[f32; 3]], attr: u32) -> BuildPoly {
        BuildPoly { verts: verts.to_vec(), attr, group: 0 }
    }
    pub fn in_group(mut self, group: u8) -> BuildPoly {
        self.group = group;
        self
    }
}

fn pad16(v: &mut Vec<u8>) {
    while !v.len().is_multiple_of(16) {
        v.push(0);
    }
}

/// The COCT file image for `polys` (polygons of one group stay together, groups in order of first use).
pub fn coct_file(polys: &[BuildPoly]) -> Vec<u8> {
    let mut groups: Vec<u8> = Vec::new();
    for p in polys {
        if !groups.contains(&p.group) {
            groups.push(p.group);
        }
    }
    let ordered: Vec<&BuildPoly> = groups.iter().flat_map(|g| polys.iter().filter(move |p| p.group == *g)).collect();
    let big: [i16; 6] = [-32767, -32767, -32767, 32767, 32767, 32767];
    let boxb = |b: &[i16; 6]| b.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>();
    let mut node = vec![0xffu8; 16];
    node.extend(boxb(&big));
    node.extend(0u16.to_le_bytes());
    node.extend((groups.len() as u16).to_le_bytes());
    let (mut meshes, mut poly_t, mut vert_t, mut plane_t, mut attr_t) = (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut first = 0usize;
    for g in &groups {
        let n = ordered.iter().filter(|p| p.group == *g).count();
        meshes.extend(boxb(&big));
        meshes.extend((first as u16).to_le_bytes());
        meshes.extend(((first + n) as u16).to_le_bytes());
        meshes.push(1);
        meshes.push(*g);
        meshes.extend(0u16.to_le_bytes());
        first += n;
    }
    for (i, p) in ordered.iter().enumerate() {
        let base = vert_t.len() / 16;
        for v in &p.verts {
            for f in [v[0], v[1], v[2], 1.0f32] {
                vert_t.extend(f.to_le_bytes());
            }
        }
        let (a, b, c) = (p.verts[0], p.verts[1], p.verts[2]);
        let (e1, e2) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
        let n = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
        let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-12);
        let n = [n[0] / l, n[1] / l, n[2] / l];
        let d = -(n[0] * a[0] + n[1] * a[1] + n[2] * a[2]);
        for f in [n[0], n[1], n[2], d] {
            plane_t.extend(f.to_le_bytes());
        }
        poly_t.push(0);
        poly_t.push(0);
        for k in 0..4 {
            let idx = if k < p.verts.len() { (base + k) as u16 } else { 0xffff };
            poly_t.extend(idx.to_le_bytes());
        }
        poly_t.extend((i as u16).to_le_bytes());
        poly_t.extend(0xffffu16.to_le_bytes());
        poly_t.extend((i as u16).to_le_bytes());
        attr_t.extend(p.attr.to_le_bytes());
    }
    let tables = [node, meshes, poly_t, vert_t, plane_t, Vec::new(), attr_t];
    let mut file = vec![0u8; 0x50];
    file[..4].copy_from_slice(b"COCT");
    for (k, t) in tables.iter().enumerate() {
        let at = file.len();
        file[0x18 + 8 * k..0x1c + 8 * k].copy_from_slice(&(at as u32).to_le_bytes());
        file[0x1c + 8 * k..0x20 + 8 * k].copy_from_slice(&(t.len() as u32).to_le_bytes());
        file.extend_from_slice(t);
        pad16(&mut file);
    }
    file
}
