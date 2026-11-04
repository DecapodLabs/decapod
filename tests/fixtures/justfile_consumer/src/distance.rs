pub fn centimeters(inches: f64) -> f64 {
    inches * 2.54
}

#[cfg(test)]
mod tests {
    use super::centimeters;

    #[test]
    fn converts_zero_negative_and_fractional_distances() {
        for (inches, expected) in [
            (0.0, 0.0),
            (1.0, 2.54),
            (-1.0, -2.54),
            (12.0, 30.48),
            (0.5, 1.27),
        ] {
            assert!((centimeters(inches) - expected).abs() < 1e-10);
        }
    }
}
