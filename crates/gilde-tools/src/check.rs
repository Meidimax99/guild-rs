use anyhow::{Result, bail};
use gilde_formats::vfs::Vfs;
use gilde_formats::{baf, bgf, res};
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
