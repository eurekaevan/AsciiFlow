use asciiflow_core::sdr_target_volume::*;
use asciiflow_core::tone_map_bt2446::SdrBt2020NonlinearRgb;

fn near(actual: f64, expected: f64, tolerance: f64, label: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{label}: {actual:.17e} != {expected:.17e}"
    );
}

#[test]
fn independent_decimal70_vectors_and_separate_xyz_path() {
    let table = include_str!("../../../tests/fixtures/tone-map/c2b-vectors.tsv");
    let mut count = 0;
    for line in table.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        assert_eq!(fields.len(), 22);
        let name = fields[0];
        let v: Vec<f64> = fields[2..].iter().map(|v| v.parse().unwrap()).collect();
        let source = if fields[1] == "bt2020_method_a_nonlinear" {
            c1_to_display_linear(SdrBt2020NonlinearRgb {
                r: v[0],
                g: v[1],
                b: v[2],
            })
            .unwrap()
        } else {
            assert_eq!(fields[1], "bt2020_linear");
            LinearBt2020SdrRgb {
                r: v[0],
                g: v[1],
                b: v[2],
            }
        };
        let xyz = bt2020_to_xyz(source).unwrap();
        let separate = xyz_to_bt709(xyz).unwrap();
        let combined = convert_bt2020_primaries(source).unwrap();
        let bounded = clip_to_bt709_target_cube(combined).unwrap();
        let nonlinear = bt709_to_nonlinear(bounded).components();
        let reconstructed = bt709_to_xyz(combined).unwrap();
        for channel in 0..3 {
            near(source.components()[channel], v[3 + channel], 5e-15, name);
            near([xyz.x, xyz.y, xyz.z][channel], v[6 + channel], 5e-15, name);
            near(separate.components()[channel], v[9 + channel], 5e-15, name);
            near(combined.components()[channel], v[9 + channel], 5e-15, name);
            near(bounded.components()[channel], v[12 + channel], 5e-15, name);
            // Inverse display power is ill-conditioned at zero. Its concave
            // Holder bound |x^a-y^a| <= |x-y|^a (0<a<1) propagates the strict
            // linear-domain oracle tolerance without hiding a matrix error.
            let linear_error = (bounded.components()[channel] - v[12 + channel]).abs();
            near(
                nonlinear[channel],
                v[15 + channel],
                linear_error.powf(1.0 / 2.4) + 1e-15,
                name,
            );
            near(
                [reconstructed.x, reconstructed.y, reconstructed.z][channel],
                [xyz.x, xyz.y, xyz.z][channel],
                5e-15,
                name,
            );
        }
        near(xyz.y, v[18], 5e-15, name);
        let after = bounded.components();
        near(
            bt709_to_xyz(LinearBt709SdrRgbUnbounded {
                r: after[0],
                g: after[1],
                b: after[2],
            })
            .unwrap()
            .y,
            v[19],
            5e-15,
            name,
        );
        count += 1;
    }
    assert_eq!(count, 41);
}

#[test]
fn signed_excursion_sweep_preserves_xyz_before_limiting() {
    for r in -8..=24 {
        for g in -8..=24 {
            for b in -8..=24 {
                let input = LinearBt2020SdrRgb {
                    r: f64::from(r) / 8.0,
                    g: f64::from(g) / 8.0,
                    b: f64::from(b) / 8.0,
                };
                let original = bt2020_to_xyz(input).unwrap();
                let converted = convert_bt2020_primaries(input).unwrap();
                let reconstructed = bt709_to_xyz(converted).unwrap();
                for (a, e) in [reconstructed.x, reconstructed.y, reconstructed.z]
                    .into_iter()
                    .zip([original.x, original.y, original.z])
                {
                    near(a, e, 5e-14, "XYZ preservation sweep");
                }
            }
        }
    }
}
