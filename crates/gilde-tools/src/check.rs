use anyhow::{Result, bail};
use gilde_formats::vfs::Vfs;
use gilde_formats::{baf, bgf, res, scene};
use std::collections::BTreeMap;

type ParseFn = fn(&[u8]) -> Result<String>;

fn parsers() -> Vec<(&'static str, ParseFn)> {
    vec![
        ("bgf", |d| {
            let b = bgf::Bgf::parse(d)?;
            if b.trailing_bytes != 0 {
                bail!("{} trailing bytes", b.trailing_bytes)
            }
            Ok(format!("{b:#?}"))
        }),
        ("baf", |d| {
            let b = baf::Baf::parse(d)?;
            if b.trailing_bytes != 0 {
                bail!("{} trailing bytes", b.trailing_bytes)
            }
            Ok(format!("{b:#?}"))
        }),
        ("res", |d| Ok(format!("{:#?}", res::ResFile::parse(d)?))),
        ("ed3", |d| Ok(format!("{:#?}", scene::Scene::parse(d)?))),
        ("ogr", |d| Ok(format!("{:#?}", scene::Group::parse(d)?))),
    ]
}

pub fn run(vfs: &Vfs, only: Option<&str>, verbose: bool) -> Result<()> {
    let mut failed_total = 0;
    for (ext, parse) in parsers() {
        if only.is_some_and(|o| !o.eq_ignore_ascii_case(ext)) {
            continue;
        }
        let paths = vfs.glob("", ext);
        let mut errors: BTreeMap<String, Vec<&str>> = BTreeMap::new();
        for p in &paths {
            if let Err(e) = vfs.read(p).map_err(anyhow::Error::from).and_then(|d| parse(&d)) {
                // Group by error text without offsets so similar failures collapse.
                let key: String = e.to_string().chars().filter(|c| !c.is_ascii_digit()).collect();
                errors.entry(key).or_default().push(p);
            }
        }
        let failed: usize = errors.values().map(Vec::len).sum();
        failed_total += failed;
        println!("{ext:>6}: {}/{} ok", paths.len() - failed, paths.len());
        for (err, ps) in &errors {
            println!("        {} × {err}  (e.g. {})", ps.len(), ps[0]);
            if verbose {
                for p in ps {
                    println!("            {p}");
                }
            }
        }
    }
    if failed_total > 0 {
        bail!("{failed_total} files failed to parse");
    }
    Ok(())
}

pub fn dump(vfs: &Vfs, path: &str) -> Result<()> {
    let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();
    let data = vfs.read(path)?;
    match parsers().into_iter().find(|(e, _)| *e == ext) {
        Some((_, parse)) => println!("{}", parse(&data)?),
        None => bail!("no parser for .{ext}"),
    }
    Ok(())
}

pub fn tree(vfs: &Vfs, path: &str) -> Result<()> {
    fn print(o: &scene::SceneObject, depth: usize) {
        let lod = o.lods.first().map(|l| format!(" lod={l}")).unwrap_or_default();
        let props: Vec<String> = o.properties.iter().map(|(k, v)| format!("{k}={v}")).collect();
        println!(
            "{:indent$}{:?} {:?} pos={:.1?} rot={:.2?}{lod} {}",
            "",
            o.kind,
            o.name,
            o.position.to_array(),
            o.rotation.to_array(),
            props.join(" "),
            indent = depth * 2
        );
        for c in &o.children {
            print(c, depth + 1);
        }
    }
    let data = vfs.read(path)?;
    let objects = if path.to_lowercase().ends_with(".ogr") {
        scene::Group::parse(&data)?.objects
    } else {
        let s = scene::Scene::parse(&data)?;
        println!("version {:#x} camera {:?} ambient {:?} props {:?}", s.version, s.camera, s.ambient, s.properties);
        if let Some(f) = &s.floor {
            println!(
                "floor {:?} stride {} heights {}B heights2 {}B water {} name2 {:?} textures {:?} params {:?} offset {:?}",
                f.name, f.stride, f.heights.len(), f.heights2.len(), f.water.len(), f.name2, f.textures, f.params, f.offset
            );
        }
        s.objects
    };
    for o in &objects {
        print(o, 0);
    }
    Ok(())
}
