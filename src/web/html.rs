//! Small HTML templates with escaping at every dynamic interpolation boundary.
//!
//! Template literals contain markup. Parenthesized values contain text (including
//! quoted attribute values), unless they are an already-rendered `HtmlFragment`.
//! Fragments compose only at markup boundaries, never inside attributes. Dynamic
//! values must not be used as tag/attribute names, scripts, CSS, or URL schemes.

use std::fmt::{self, Display, Write};

/// Rendered markup. No `Display`, `From<String>`, or mutable string access is
/// provided: runtime strings can enter a template only as escaped text.
#[derive(Default)]
pub(crate) struct HtmlFragment(String);

impl HtmlFragment {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Literal markup or an embedded, repository-owned static stylesheet only.
    pub(crate) fn literal(value: &'static str) -> Self {
        Self(value.to_owned())
    }

    pub(crate) fn push_literal(&mut self, value: &'static str) {
        self.0.push_str(value);
    }

    pub(crate) fn push(&mut self, value: &impl HtmlValue) {
        value.write_html(&mut self.0);
    }

    pub(crate) fn into_string(self) -> String {
        self.0
    }
}

impl FromIterator<HtmlFragment> for HtmlFragment {
    fn from_iter<T: IntoIterator<Item = HtmlFragment>>(iter: T) -> Self {
        let mut output = Self::new();
        for fragment in iter {
            output.push(&fragment);
        }
        output
    }
}

pub(crate) trait HtmlValue {
    fn write_html(&self, output: &mut String);
}

impl HtmlValue for HtmlFragment {
    fn write_html(&self, output: &mut String) {
        output.push_str(&self.0);
    }
}

impl HtmlValue for &HtmlFragment {
    fn write_html(&self, output: &mut String) {
        output.push_str(&self.0);
    }
}

impl<T: Display + ?Sized> HtmlValue for T {
    fn write_html(&self, output: &mut String) {
        // Escape while formatting, so custom Display values cannot bypass the
        // same boundary used for strings and numeric/date values.
        write!(EscapingWriter(output), "{self}").expect("writing to a String cannot fail");
    }
}

struct EscapingWriter<'a>(&'a mut String);

impl Write for EscapingWriter<'_> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        for character in value.chars() {
            match character {
                '&' => self.0.push_str("&amp;"),
                '<' => self.0.push_str("&lt;"),
                '>' => self.0.push_str("&gt;"),
                '"' => self.0.push_str("&quot;"),
                '\'' => self.0.push_str("&#39;"),
                other => self.0.push(other),
            }
        }
        Ok(())
    }
}

/// Literal markup alternates with parenthesized, automatically escaped values.
/// Unlike `format!`, there is no implicit capture or unchecked string insertion.
macro_rules! html {
    (@parts $output:ident;) => {};
    (@parts $output:ident; $literal:literal $($rest:tt)*) => {
        $output.push_literal($literal);
        $crate::web::html::html!(@parts $output; $($rest)*);
    };
    (@parts $output:ident; ($value:expr) $($rest:tt)*) => {
        $output.push(&$value);
        $crate::web::html::html!(@parts $output; $($rest)*);
    };
    ($($parts:tt)*) => {{
        let mut output = $crate::web::html::HtmlFragment::new();
        $crate::web::html::html!(@parts output; $($parts)*);
        output
    }};
}

pub(crate) use html;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_attributes_and_display_values_are_escaped_once() {
        struct UntrustedDisplay;
        impl Display for UntrustedDisplay {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("<script>\"'&amp;")
            }
        }
        let fragment = html! { "<span>" (UntrustedDisplay) "</span>" };
        let page = html! { "<input value=\"" ("\"'&<>") "\">" (fragment) };
        assert_eq!(
            page.into_string(),
            "<input value=\"&quot;&#39;&amp;&lt;&gt;\"><span>&lt;script&gt;&quot;&#39;&amp;amp;</span>"
        );
    }
}
