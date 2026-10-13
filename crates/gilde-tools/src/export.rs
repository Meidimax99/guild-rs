use anyhow::Result;
use gilde_formats::bgf::Bgf;
use gilde_formats::vfs::Vfs;
use std::fmt::Write as _;
use std::path::Path;

/// Write `<name>.obj` + `<name>.mtl` and copy referenced textures next to them.
pub fn obj(vfs: &Vfs, path: &str, out_dir: &Path) -> Result<()> {
    let bgf = Bgf::parse(&vfs.read(path)?)?;
    std::fs::create_dir_all(out_dir)?;
    let stem = path.rsplit('/').next().unwrap().trim_start_matches('*');
    let stem = stem.rsplit_once('.').map_or(stem, |s| s.0);
    let mut mtl = String::new();
    for (i, m) in bgf.materials.iter().enumerate() {
        writeln!(mtl, "newmtl m{i}\nKd 1 1 1")?;
        if !m.texture.is_empty() {
            let tex = format!("{}.bmp", m.texture);
            if let Ok(data) = vfs.read(&format!("textures/*{tex}")) {
                std::fs::write(out_dir.join(&tex), data)?;
            }
            writeln!(mtl, "map_Kd {tex}")?;
        }
    }
    std::fs::write(out_dir.join(format!("{stem}.mtl")), mtl)?;

    let mesh = bgf.merged();
    let mut o = format!("mtllib {stem}.mtl\n");
    for v in &mesh.vertices {
        writeln!(o, "v {} {} {}", v.x, v.y, v.z)?;
    }
    for f in &mesh.faces {
        for uv in f.uvs {
            writeln!(o, "vt {} {}", uv.x, 1.0 - uv.y)?;
        }
    }
    let mut cur = None;
    for (i, f) in mesh.faces.iter().enumerate() {
        if f.material != cur {
            cur = f.material;
            writeln!(o, "usemtl m{}", cur.unwrap_or(0))?;
        }
        let t = i * 3 + 1;
        let [a, b, c] = f.indices.map(|x| x + 1);
        writeln!(o, "f {a}/{t} {b}/{} {c}/{}", t + 1, t + 2)?;
    }
    std::fs::write(out_dir.join(format!("{stem}.obj")), o)?;
    println!("wrote {} vertices, {} faces", mesh.vertices.len(), mesh.faces.len());
    Ok(())
}
