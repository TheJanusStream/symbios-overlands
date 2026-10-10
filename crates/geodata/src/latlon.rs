//! Latitude and longitude for the map's grid (#1599): a square's middle as
//! the geographic coordinates anyone can read off a web map, rather than as
//! the eastings and northings the data is served in.
//!
//! ETRS89 / UTM zone 33N (EPSG:25833) is the transverse Mercator projection
//! of the GRS80 ellipsoid about the meridian 15 degrees east, scaled 0.9996,
//! with a false easting of 500 km. Its geographic coordinates are ETRS89's,
//! which in Berlin lie about a metre from the WGS84 a phone's GPS or a web
//! map shows (the continent drifts some 2.5 cm a year against WGS84): close
//! enough for a square on whole metres, though not a surveyor's figure.
//!
//! The projection is Krueger's series in the ellipsoid's third flattening,
//! to its third power (the form Wikipedia's "Universal Transverse Mercator
//! coordinate system" gives): within a millimetre of Snyder's independent
//! series (USGS Professional Paper 1395) across Berlin, which the tests
//! hold it to. [`libm`] does the arithmetic, as for the rest of this crate.

/// GRS80's semi-major axis (m).
const A: f64 = 6_378_137.0;

/// GRS80's flattening.
const F: f64 = 1.0 / 298.257_222_101;

/// The zone's scale on its central meridian.
const K0: f64 = 0.9996;

/// The zone's central meridian (degrees east).
const LON0: f64 = 15.0;

/// The zone's false easting (m).
const FALSE_EASTING: f64 = 500_000.0;

/// The series' constants, from the third flattening `n`.
struct Series {
    /// The rectifying radius, scaled by [`K0`] (m).
    radius: f64,
    /// The first eccentricity.
    e: f64,
    alpha: [f64; 3],
    beta: [f64; 3],
    delta: [f64; 3],
}

fn series() -> Series {
    let n = F / (2.0 - F);
    let (n2, n3) = (n * n, n * n * n);
    Series {
        radius: K0 * A / (1.0 + n) * (1.0 + n2 / 4.0 + n2 * n2 / 64.0),
        e: 2.0 * libm::sqrt(n) / (1.0 + n),
        alpha: [
            n / 2.0 - 2.0 * n2 / 3.0 + 5.0 * n3 / 16.0,
            13.0 * n2 / 48.0 - 3.0 * n3 / 5.0,
            61.0 * n3 / 240.0,
        ],
        beta: [
            n / 2.0 - 2.0 * n2 / 3.0 + 37.0 * n3 / 96.0,
            n2 / 48.0 + n3 / 15.0,
            17.0 * n3 / 480.0,
        ],
        delta: [
            2.0 * n - 2.0 * n2 / 3.0 - 2.0 * n3,
            7.0 * n2 / 3.0 - 8.0 * n3 / 5.0,
            56.0 * n3 / 15.0,
        ],
    }
}

/// The grid point - easting and northing (m) - of the point at latitude
/// `lat` and longitude `lon` (degrees, ETRS89). Sound within some tens of
/// degrees of the zone's meridian; a caller far from it clamps first.
pub fn to_grid(lat: f64, lon: f64) -> (f64, f64) {
    let s = series();
    let phi = lat.to_radians();
    let lambda = (lon - LON0).to_radians();
    let sin_phi = libm::sin(phi);
    // The conformal latitude's tangent.
    let t = libm::sinh(libm::atanh(sin_phi) - s.e * libm::atanh(s.e * sin_phi));
    let xi = libm::atan2(t, libm::cos(lambda));
    let eta = libm::atanh(libm::sin(lambda) / libm::sqrt(1.0 + t * t));
    let (mut east, mut north) = (eta, xi);
    for (j, alpha) in s.alpha.iter().enumerate() {
        let k = 2.0 * (j + 1) as f64;
        east += alpha * libm::cos(k * xi) * libm::sinh(k * eta);
        north += alpha * libm::sin(k * xi) * libm::cosh(k * eta);
    }
    (FALSE_EASTING + s.radius * east, s.radius * north)
}

/// The latitude and longitude (degrees, ETRS89) of the grid point at
/// easting `e` and northing `n` (m).
pub fn to_lat_lon(e: f64, n: f64) -> (f64, f64) {
    let s = series();
    let (xi, eta) = (n / s.radius, (e - FALSE_EASTING) / s.radius);
    let (mut xi_p, mut eta_p) = (xi, eta);
    for (j, beta) in s.beta.iter().enumerate() {
        let k = 2.0 * (j + 1) as f64;
        xi_p -= beta * libm::sin(k * xi) * libm::cosh(k * eta);
        eta_p -= beta * libm::cos(k * xi) * libm::sinh(k * eta);
    }
    // The conformal latitude, then the geodetic.
    let chi = libm::asin(libm::sin(xi_p) / libm::cosh(eta_p));
    let mut phi = chi;
    for (j, delta) in s.delta.iter().enumerate() {
        phi += delta * libm::sin(2.0 * (j + 1) as f64 * chi);
    }
    let lambda = libm::atan2(libm::sinh(eta_p), libm::cos(xi_p));
    (phi.to_degrees(), LON0 + lambda.to_degrees())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Points round Berlin, and where Snyder's series puts them on the
    /// grid: a formula of another family, so agreement says both are right.
    const SNYDER: [((f64, f64), (f64, f64)); 4] = [
        // The Brandenburg Gate.
        ((52.516_275, 13.377_704), (389_918.042, 5_819_699.133)),
        // The television tower.
        ((52.520_803, 13.409_430), (392_081.783, 5_820_154.838)),
        // Spandau, in the far west.
        ((52.53, 13.10), (371_116.097, 5_821_685.315)),
        // Koepenick, in the far south-east.
        ((52.38, 13.70), (411_513.863, 5_804_099.445)),
    ];

    #[test]
    fn berlin_lies_where_snyders_series_puts_it() {
        for ((lat, lon), (e, n)) in SNYDER {
            let (ge, gn) = to_grid(lat, lon);
            assert!(
                (ge - e).abs() < 0.01 && (gn - n).abs() < 0.01,
                "({lat}, {lon}) went to ({ge:.3}, {gn:.3}), not ({e}, {n})"
            );
            let (blat, blon) = to_lat_lon(e, n);
            // A centimetre is about 1e-7 degrees.
            assert!(
                (blat - lat).abs() < 2e-7 && (blon - lon).abs() < 2e-7,
                "({e}, {n}) came back as ({blat}, {blon}), not ({lat}, {lon})"
            );
        }
    }

    /// Across Berlin's coverage, a grid point taken to latitude and
    /// longitude and back lands within a millimetre of where it started.
    #[test]
    fn a_grid_point_comes_back_within_a_millimetre() {
        for e in (365_000..=420_000).step_by(5_000) {
            for n in (5_798_000..=5_842_000).step_by(4_000) {
                let (lat, lon) = to_lat_lon(f64::from(e), f64::from(n));
                let (be, bn) = to_grid(lat, lon);
                assert!(
                    (be - f64::from(e)).abs() < 1e-3 && (bn - f64::from(n)).abs() < 1e-3,
                    "({e}, {n}) came back as ({be}, {bn})"
                );
            }
        }
    }

    /// The zone's meridian is its false easting, and the equator its zero
    /// northing.
    #[test]
    fn the_zones_meridian_and_the_equator_are_its_axes() {
        let (e, n) = to_grid(0.0, LON0);
        assert!(
            (e - FALSE_EASTING).abs() < 1e-6 && n.abs() < 1e-6,
            "({e}, {n})"
        );
        let (_, n) = to_grid(52.0, LON0);
        let (lat, lon) = to_lat_lon(FALSE_EASTING, n);
        // Within a millimetre, about 1e-8 degrees: the series' own error.
        assert!(
            (lat - 52.0).abs() < 1e-8 && (lon - LON0).abs() < 1e-12,
            "({lat}, {lon})"
        );
    }
}
