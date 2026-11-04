pub fn fahrenheit(celsius: f64) -> f64 {
    celsius * 9.0 / 5.0 + 32.0
}

#[cfg(test)]
mod tests {
    use super::fahrenheit;

    #[test]
    fn converts_freezing_boiling_negative_and_fractional_temperatures() {
        for (celsius, expected) in [
            (0.0, 32.0),
            (100.0, 212.0),
            (-40.0, -40.0),
            (37.0, 98.6),
            (0.5, 32.9),
        ] {
            assert!((fahrenheit(celsius) - expected).abs() < 1e-10);
        }
    }
}
