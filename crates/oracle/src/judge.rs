use craft_compare::{Image, ImageTolerance, Verdict, compare_bytes, image::compare_images, xml::compare_xml};

/// Decides whether the new output is acceptable relative to the old one.
pub trait Judge<O> {
    fn judge(&self, old: &O, new: &O) -> Verdict;
}

/// Exact byte equality. A difference is `Exceeds { metric: "first_diff", value: offset, limit: 0 }`.
#[derive(Clone, Copy, Debug, Default)]
pub struct BytesJudge;

impl<O: AsRef<[u8]>> Judge<O> for BytesJudge {
    fn judge(&self, old: &O, new: &O) -> Verdict {
        match compare_bytes(old.as_ref(), new.as_ref()).first_diff {
            None => Verdict::Identical,
            Some(at) => Verdict::Exceeds { metric: "first_diff".into(), value: at as f64, limit: 0.0 },
        }
    }
}

/// Canonical XML equality. A difference is metric `xml`; unparseable output is `xml_parse`.
#[derive(Clone, Copy, Debug, Default)]
pub struct XmlJudge;

impl<O: AsRef<str>> Judge<O> for XmlJudge {
    fn judge(&self, old: &O, new: &O) -> Verdict {
        match compare_xml(old.as_ref(), new.as_ref()) {
            Ok(None) => Verdict::Identical,
            Ok(Some(_)) => Verdict::Exceeds { metric: "xml".into(), value: 1.0, limit: 0.0 },
            Err(_) => Verdict::Exceeds { metric: "xml_parse".into(), value: 1.0, limit: 0.0 },
        }
    }
}

/// Image metrics against an [`ImageTolerance`]. Mismatched shapes are metric `image_shape`.
#[derive(Clone, Copy, Debug)]
pub struct ImageJudge {
    pub tolerance: ImageTolerance,
    /// Bad-pixel threshold in the `0..=1` sample scale.
    pub bad_pixel_threshold: f64,
}

impl ImageJudge {
    pub fn new(tolerance: ImageTolerance) -> Self {
        Self { tolerance, bad_pixel_threshold: 1.0 / 255.0 }
    }
}

impl Judge<Image> for ImageJudge {
    fn judge(&self, old: &Image, new: &Image) -> Verdict {
        match compare_images(old, new, self.bad_pixel_threshold) {
            Ok(r) => self.tolerance.verdict(&r),
            Err(_) => Verdict::Exceeds { metric: "image_shape".into(), value: 1.0, limit: 0.0 },
        }
    }
}

/// A [`Judge`] from a closure.
pub struct FnJudge<F>(pub F);

impl<O, F: Fn(&O, &O) -> Verdict> Judge<O> for FnJudge<F> {
    fn judge(&self, old: &O, new: &O) -> Verdict {
        (self.0)(old, new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_judge() {
        assert_eq!(BytesJudge.judge(&vec![1u8, 2], &vec![1, 2]), Verdict::Identical);
        let v = BytesJudge.judge(&vec![1u8, 2], &vec![1, 9]);
        assert_eq!(v, Verdict::Exceeds { metric: "first_diff".into(), value: 1.0, limit: 0.0 });
    }

    #[test]
    fn xml_judge_ignores_attr_order() {
        let j = XmlJudge;
        assert_eq!(j.judge(&"<a x=\"1\" y=\"2\"/>".to_string(), &"<a y=\"2\" x=\"1\"/>".to_string()), Verdict::Identical);
        assert!(!j.judge(&"<a/>".to_string(), &"<b/>".to_string()).passed());
        assert!(!j.judge(&"<a".to_string(), &"<a/>".to_string()).passed());
    }

    #[test]
    fn image_judge_tolerance_and_shape() {
        let a = Image::from_plane(16, 16, vec![0.5; 256]).unwrap();
        let b = Image::from_plane(16, 16, vec![0.5 + 0.004; 256]).unwrap();
        let j = ImageJudge::new(ImageTolerance { max_abs: Some(0.01), ..Default::default() });
        assert_eq!(j.judge(&a, &a), Verdict::Identical);
        assert!(matches!(j.judge(&a, &b), Verdict::Within { .. }));
        let tight = ImageJudge::new(ImageTolerance { max_abs: Some(0.001), ..Default::default() });
        assert!(!tight.judge(&a, &b).passed());
        let c = Image::from_plane(8, 8, vec![0.5; 64]).unwrap();
        assert!(matches!(j.judge(&a, &c), Verdict::Exceeds { metric, .. } if metric == "image_shape"));
    }

    #[test]
    fn closure_judge() {
        let j = FnJudge(|a: &i32, b: &i32| Verdict::at_most("delta", f64::from((a - b).abs()), 2.0));
        assert_eq!(j.judge(&1, &1), Verdict::Identical);
        assert!(j.judge(&1, &2).passed());
        assert!(!j.judge(&1, &9).passed());
    }
}
