pub struct Stylized<'a> {
    s: &'a str,
    code: &'a str,
    bold: bool,
}

impl<'a> Stylized<'a> {
    pub fn new(s: &'a str, code: &'a str) -> Self {
        Self { s, code, bold: false }
    }
    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }
}

pub trait AnsiExt {
    fn bright_cyan(&self) -> Stylized<'_>;
    fn bright_white(&self) -> Stylized<'_>;
    fn bright_black(&self) -> Stylized<'_>;
    fn bright_yellow(&self) -> Stylized<'_>;
    fn bright_green(&self) -> Stylized<'_>;
    fn bright_red(&self) -> Stylized<'_>;
    fn bright_blue(&self) -> Stylized<'_>;
    fn bright_magenta(&self) -> Stylized<'_>;
    fn cyan(&self) -> Stylized<'_>;
    fn green(&self) -> Stylized<'_>;
}

impl AnsiExt for str {
    fn bright_cyan(&self) -> Stylized<'_> { Stylized::new(self, "\x1b[96m") }
    fn bright_white(&self) -> Stylized<'_> { Stylized::new(self, "\x1b[97m") }
    fn bright_black(&self) -> Stylized<'_> { Stylized::new(self, "\x1b[90m") }
    fn bright_yellow(&self) -> Stylized<'_> { Stylized::new(self, "\x1b[93m") }
    fn bright_green(&self) -> Stylized<'_> { Stylized::new(self, "\x1b[92m") }
    fn bright_red(&self) -> Stylized<'_> { Stylized::new(self, "\x1b[91m") }
    fn bright_blue(&self) -> Stylized<'_> { Stylized::new(self, "\x1b[94m") }
    fn bright_magenta(&self) -> Stylized<'_> { Stylized::new(self, "\x1b[95m") }
    fn cyan(&self) -> Stylized<'_> { Stylized::new(self, "\x1b[36m") }
    fn green(&self) -> Stylized<'_> { Stylized::new(self, "\x1b[32m") }
}

impl<'a> std::fmt::Display for Stylized<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.bold {
            write!(f, "{}\x1b[1m{}\x1b[0m", self.code, self.s)
        } else {
            write!(f, "{}{}\x1b[0m", self.code, self.s)
        }
    }
}

pub fn bold(s: &str) -> String {
    format!("\x1b[1m{}\x1b[0m", s)
}
