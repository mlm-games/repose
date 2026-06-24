use std::cmp::Ordering;

use crate::slug::outline::QuadCurve;

pub const NUM_BANDS: u32 = 8;

#[derive(Clone)]
pub struct BandList {
    pub indices: Vec<u32>,
}

pub struct BandData {
    pub bounds: [f32; 4],
    pub h_bands: Vec<BandList>,
    pub v_bands: Vec<BandList>,
    pub h_count: u32,
    pub v_count: u32,
    pub band_scale: [f32; 2],
    pub band_offset: [f32; 2],
}

pub fn build_bands(curves: &[QuadCurve]) -> BandData {
    if curves.is_empty() {
        return BandData {
            bounds: [0.0; 4],
            h_bands: Vec::new(),
            v_bands: Vec::new(),
            h_count: 1,
            v_count: 1,
            band_scale: [1.0, 1.0],
            band_offset: [0.0, 0.0],
        };
    }

    let mut xmin = f32::MAX;
    let mut ymin = f32::MAX;
    let mut xmax = f32::MIN;
    let mut ymax = f32::MIN;
    for c in curves {
        xmin = xmin.min(c.p1[0]).min(c.p2[0]).min(c.p3[0]);
        ymin = ymin.min(c.p1[1]).min(c.p2[1]).min(c.p3[1]);
        xmax = xmax.max(c.p1[0]).max(c.p2[0]).max(c.p3[0]);
        ymax = ymax.max(c.p1[1]).max(c.p2[1]).max(c.p3[1]);
    }
    let pad = 0.01 * (xmax - xmin).max(ymax - ymin).max(1.0);
    xmin -= pad;
    ymin -= pad;
    xmax += pad;
    ymax += pad;

    let bw = (xmax - xmin).max(1e-6);
    let bh = (ymax - ymin).max(1e-6);
    let inv_bw = 1.0 / bw;
    let inv_bh = 1.0 / bh;

    let band_scale = [NUM_BANDS as f32 * inv_bw, NUM_BANDS as f32 * inv_bh];
    let band_offset = [-xmin * band_scale[0], -ymin * band_scale[1]];

    let mut h_lists: Vec<Vec<u32>> = (0..NUM_BANDS).map(|_| Vec::new()).collect();
    let mut v_lists: Vec<Vec<u32>> = (0..NUM_BANDS).map(|_| Vec::new()).collect();

    for (i, c) in curves.iter().enumerate() {
        let i = i as u32;
        let cymin = c.p1[1].min(c.p2[1]).min(c.p3[1]);
        let cymax = c.p1[1].max(c.p2[1]).max(c.p3[1]);
        let cxmin = c.p1[0].min(c.p2[0]).min(c.p3[0]);
        let cxmax = c.p1[0].max(c.p2[0]).max(c.p3[0]);

        // Horizontal bands (y-axis split).
        let vs = ((cymin - ymin) * NUM_BANDS as f32 * inv_bh)
            .floor()
            .max(0.0)
            .min((NUM_BANDS - 1) as f32) as u32;
        let ve = ((cymax - ymin) * NUM_BANDS as f32 * inv_bh)
            .ceil()
            .max(0.0)
            .min(NUM_BANDS as f32) as u32;
        for b in vs..ve {
            h_lists[b as usize].push(i);
        }

        // Vertical bands (x-axis split)
        let hs = ((cxmin - xmin) * NUM_BANDS as f32 * inv_bw)
            .floor()
            .max(0.0)
            .min((NUM_BANDS - 1) as f32) as u32;
        let he = ((cxmax - xmin) * NUM_BANDS as f32 * inv_bw)
            .ceil()
            .max(0.0)
            .min(NUM_BANDS as f32) as u32;
        for b in hs..he {
            v_lists[b as usize].push(i);
        }
    }

    let cmp_desc = |a: &u32, b: &u32, coord: fn(&QuadCurve) -> f32| -> Ordering {
        coord(&curves[*b as usize]).total_cmp(&coord(&curves[*a as usize]))
    };
    for list in h_lists.iter_mut() {
        list.sort_by(|a, b| cmp_desc(a, b, |c| c.p1[0].max(c.p2[0]).max(c.p3[0])));
    }
    for list in v_lists.iter_mut() {
        list.sort_by(|a, b| cmp_desc(a, b, |c| c.p1[1].max(c.p2[1]).max(c.p3[1])));
    }

    // Always write all NUM_BANDS bands so the shader's full-range band_scale
    // maps correctly. Empty bands get count=0 and are harmless.
    let h_count = NUM_BANDS;
    let v_count = NUM_BANDS;

    BandData {
        bounds: [xmin, ymin, xmax, ymax],
        h_bands: h_lists
            .into_iter()
            .map(|indices| BandList { indices })
            .collect(),
        v_bands: v_lists
            .into_iter()
            .map(|indices| BandList { indices })
            .collect(),
        h_count,
        v_count,
        band_scale,
        band_offset,
    }
}
