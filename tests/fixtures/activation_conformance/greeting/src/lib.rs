pub fn greeting() -> &'static str {
    "Hello, fixture!"
}

#[cfg(test)]
mod tests {
    #[test]
    fn accepted_greeting() {
        assert_eq!(super::greeting(), "Hello, governed fixture!");
    }
}
