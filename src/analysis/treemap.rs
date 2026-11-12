/// Axis-aligned rectangle in normalized [0,1] coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }
    pub fn area(&self) -> f64 {
        self.w.max(0.0) * self.h.max(0.0)
    }
    pub fn right(&self) -> f64 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }
}

/// Squarified treemap layout. Input sizes are laid out largest-first; the
/// returned rectangles are in the same order as the input. The total area is
/// normalized to one, so rectangle area / total area == size / total size.
pub fn squarify(sizes: &[f64]) -> Vec<Rect> {
    let n = sizes.len();
    let mut out = vec![Rect::default(); n];
    if n == 0 {
        return out;
    }
    let positive: Vec<usize> = (0..n).filter(|&i| sizes[i].is_finite() && sizes[i] > 0.0).collect();
    if positive.is_empty() {
        return out;
    }
    let total: f64 = positive.iter().map(|&i| sizes[i]).sum();
    if total <= 0.0 {
        return out;
    }
    let values: Vec<f64> = positive.iter().map(|&i| sizes[i] / total).collect();
    layout(&values, &positive, Rect::new(0.0, 0.0, 1.0, 1.0), &mut out);
    out
}

fn layout(values: &[f64], ids: &[usize], mut rect: Rect, out: &mut [Rect]) {
    let mut start = 0usize;
    while start < values.len() {
        if rect.w <= 1e-12 || rect.h <= 1e-12 {
            for &id in &ids[start..] {
                out[id] = Rect::new(rect.x, rect.y, 0.0, 0.0);
            }
            break;
        }
        let remaining: f64 = values[start..].iter().sum();
        if remaining <= 0.0 {
            break;
        }
        let scale = rect.area() / remaining;
        let side = rect.w.min(rect.h);

        let mut row_areas: Vec<f64> = vec![values[start] * scale];
        let mut end = start + 1;
        let mut best = worst_ratio(&row_areas, side);
        while end < values.len() {
            let area = values[end] * scale;
            if area <= 0.0 {
                end += 1;
                continue;
            }
            row_areas.push(area);
            let candidate = worst_ratio(&row_areas, side);
            if candidate <= best || !candidate.is_finite() {
                best = candidate;
                end += 1;
            } else {
                row_areas.pop();
                break;
            }
        }

        let row_sum: f64 = row_areas.iter().sum();
        if row_sum <= 0.0 {
            break;
        }
        if rect.w >= rect.h {
            let thickness = (row_sum / rect.h).min(rect.w);
            let mut y = rect.y;
            let count = row_areas.len();
            for (i, area) in row_areas.iter().enumerate() {
                let h = if i + 1 == count {
                    rect.bottom() - y
                } else {
                    (area / thickness).min(rect.bottom() - y)
                };
                let id = ids[start + i];
                out[id] = Rect::new(rect.x, y, thickness, h.max(0.0));
                y += h.max(0.0);
            }
            rect.x += thickness;
            rect.w = (rect.w - thickness).max(0.0);
        } else {
            let thickness = (row_sum / rect.w).min(rect.h);
            let mut x = rect.x;
            let count = row_areas.len();
            for (i, area) in row_areas.iter().enumerate() {
                let w = if i + 1 == count {
                    rect.right() - x
                } else {
                    (area / thickness).min(rect.right() - x)
                };
                let id = ids[start + i];
                out[id] = Rect::new(x, rect.y, w.max(0.0), thickness);
                x += w.max(0.0);
            }
            rect.y += thickness;
            rect.h = (rect.h - thickness).max(0.0);
        }
        start = end;
    }
}

fn worst_ratio(areas: &[f64], side: f64) -> f64 {
    let sum: f64 = areas.iter().sum();
    if sum <= 0.0 || side <= 0.0 {
        return f64::INFINITY;
    }
    let max = areas.iter().cloned().fold(0.0f64, f64::max);
    let min = areas.iter().cloned().fold(f64::INFINITY, f64::min);
    if max <= 0.0 || min <= 0.0 || !min.is_finite() {
        return f64::INFINITY;
    }
    let s2 = sum * sum;
    let side2 = side * side;
    (side2 * max / s2).max(s2 / (side2 * min))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overlap(a: Rect, b: Rect) -> bool {
        let eps = 1e-9;
        a.x + eps < b.right()
            && b.x + eps < a.right()
            && a.y + eps < b.bottom()
            && b.y + eps < a.bottom()
    }

    #[test]
    fn single_item_fills_area() {
        let r = squarify(&[100.0]);
        assert!((r[0].area() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn zero_sizes_are_ignored() {
        let r = squarify(&[0.0, 10.0, 0.0]);
        assert_eq!(r[0].area(), 0.0);
        assert!((r[1].area() - 1.0).abs() < 1e-9);
        assert_eq!(r[2].area(), 0.0);
    }

    #[test]
    fn areas_are_proportional_and_inside_bounds() {
        let sizes = [500.0, 250.0, 120.0, 60.0, 30.0, 15.0, 8.0, 4.0, 2.0, 1.0];
        let total: f64 = sizes.iter().sum();
        let rects = squarify(&sizes);
        let area_sum: f64 = rects.iter().map(|r| r.area()).sum();
        assert!((area_sum - 1.0).abs() < 1e-6);
        for (r, size) in rects.iter().zip(sizes.iter()) {
            assert!((r.area() - size / total).abs() < 1e-6, "{r:?} vs {size}");
            assert!(r.x >= -1e-9 && r.y >= -1e-9);
            assert!(r.right() <= 1.0 + 1e-9 && r.bottom() <= 1.0 + 1e-9);
        }
        for i in 0..rects.len() {
            for j in i + 1..rects.len() {
                assert!(!overlap(rects[i], rects[j]), "{:?} {:?}", rects[i], rects[j]);
            }
        }
    }

    #[test]
    fn many_uneven_items() {
        let sizes: Vec<f64> = (0..500).map(|i| (i * i + 1) as f64).collect();
        let rects = squarify(&sizes);
        let area_sum: f64 = rects.iter().map(|r| r.area()).sum();
        assert!((area_sum - 1.0).abs() < 1e-4);
        assert!(rects.iter().all(|r| r.x >= -1e-9 && r.right() <= 1.0 + 1e-9));
        assert!(rects.iter().all(|r| r.y >= -1e-9 && r.bottom() <= 1.0 + 1e-9));
    }

    #[test]
    fn handles_nan_and_negative() {
        let r = squarify(&[f64::NAN, -5.0, 10.0]);
        assert_eq!(r[0].area(), 0.0);
        assert_eq!(r[1].area(), 0.0);
        assert!((r[2].area() - 1.0).abs() < 1e-9);
    }
}
