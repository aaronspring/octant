//! # fetch_coastlines
//!
//! A one-shot data preparation tool that downloads Natural Earth coastline GeoJSON
//! files from the official `nvkelso/natural-earth-vector` GitHub repository and
//! converts them into compact binary vertex buffers ready for GPU upload.

#[cfg(target_arch = "wasm32")]
fn main() {}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    desktop::run();
}

#[cfg(not(target_arch = "wasm32"))]
mod desktop {
    use std::{
        fs,
        io::{self, Write},
        path::Path,
        time::Instant,
    };

    #[derive(serde::Deserialize)]
    struct FeatureCollection {
        features: Vec<Feature>,
    }

    #[derive(serde::Deserialize)]
    struct Feature {
        geometry: Option<Geometry>,
    }

    #[derive(serde::Deserialize)]
    struct Geometry {
        #[serde(rename = "type")]
        kind: String,
        coordinates: serde_json::Value,
    }

    struct Scale {
        name: &'static str,
        url: &'static str,
    }

    const SCALES: &[Scale] = &[
        Scale {
            name: "110m",
            url: "https://raw.githubusercontent.com/nvkelso/natural-earth-vector/master/geojson/ne_110m_coastline.geojson",
        },
        Scale {
            name: "50m",
            url: "https://raw.githubusercontent.com/nvkelso/natural-earth-vector/master/geojson/ne_50m_coastline.geojson",
        },
        Scale {
            name: "10m",
            url: "https://raw.githubusercontent.com/nvkelso/natural-earth-vector/master/geojson/ne_10m_coastline.geojson",
        },
    ];

    pub fn run() {
        let out_dir = Path::new("assets/coastlines");
        if let Err(e) = fs::create_dir_all(out_dir) {
            eprintln!("Failed to create '{}': {e}", out_dir.display());
            std::process::exit(1);
        }

        for scale in SCALES {
            let t0 = Instant::now();
            print!("Fetching {} coastline ... ", scale.name);
            io::stdout().flush().ok();

            match fetch_and_convert(scale, out_dir) {
                Ok((pair_count, byte_len)) => {
                    println!(
                        "done in {:.1}s  —  {pair_count} vertex pairs, {:.1} KB on disk",
                        t0.elapsed().as_secs_f32(),
                        byte_len as f32 / 1024.0,
                    );
                }
                Err(e) => eprintln!("ERROR: {e}"),
            }
        }
    }

    fn fetch_and_convert(
        scale: &Scale,
        out_dir: &Path,
    ) -> Result<(usize, usize), Box<dyn std::error::Error>> {
        let body = reqwest::blocking::get(scale.url)?
            .error_for_status()?
            .text()?;

        let collection: FeatureCollection = serde_json::from_str(&body)?;
        let mut verts: Vec<f32> = Vec::with_capacity(1 << 16);
        let mut pair_count = 0usize;

        for feature in &collection.features {
            let Some(geom) = &feature.geometry else {
                continue;
            };

            match geom.kind.as_str() {
                "LineString" => {
                    push_line_string(&geom.coordinates, &mut verts, &mut pair_count);
                    verts.push(f32::NAN);
                    verts.push(f32::NAN);
                }
                "MultiLineString" => {
                    let Some(rings) = geom.coordinates.as_array() else {
                        continue;
                    };
                    for ring in rings {
                        push_line_string(ring, &mut verts, &mut pair_count);
                        verts.push(f32::NAN);
                        verts.push(f32::NAN);
                    }
                }
                other => {
                    eprintln!("  warning: skipping unexpected geometry type '{other}'");
                }
            }
        }

        let out_path = out_dir.join(format!("coastline_{}.bin", scale.name));
        let raw_bytes: &[u8] = bytemuck::cast_slice(&verts);
        fs::write(&out_path, raw_bytes)?;

        Ok((pair_count, raw_bytes.len()))
    }

    fn push_line_string(coords: &serde_json::Value, verts: &mut Vec<f32>, pair_count: &mut usize) {
        let Some(points) = coords.as_array() else {
            return;
        };
        for pt in points {
            let Some(xy) = pt.as_array() else { continue };
            let lon = xy.first().and_then(|v| v.as_f64());
            let lat = xy.get(1).and_then(|v| v.as_f64());
            let (Some(lon), Some(lat)) = (lon, lat) else {
                continue;
            };
            verts.push(lon as f32);
            verts.push(lat as f32);
            *pair_count += 1;
        }
    }
}
