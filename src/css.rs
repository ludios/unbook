use clap::ValueEnum;
use csscolorparser::Color;
use indoc::formatdoc;
use once_cell::sync::Lazy;
use regex::{Regex, Captures};
use std::{collections::{HashMap, HashSet}, borrow::Cow};
use crate::font::{classify_font_family, GenericFontFamily};

#[derive(ValueEnum, Copy, Clone, Debug)]
#[allow(non_camel_case_types)]
pub(crate) enum FontFamilyReplacementMode {
    never,
    if_one,
    always,
}

pub(crate) struct FontReplacementOptions {
    pub min_font_size: String,
    pub base_font_size: String,
    pub base_font_family: String,
    pub monospace_font_family: String,
    pub replace_serif_and_sans_serif: FontFamilyReplacementMode,
    pub replace_monospace: FontFamilyReplacementMode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Ruleset {
    pub selectors: String,
    pub declaration_block: String,
}

impl ToString for Ruleset {
    fn to_string(&self) -> String {
        format!("{} {{\n    {}\n}}\n", self.selectors, self.declaration_block)
    }
}

// Copied from https://github.com/qryxip/snowchains/blob/dcd76c1dbb87eea239ba17f28b44ee11fdd3fd80/src/macros.rs

/// Return a Lazy<Regex> for the given regexp string
macro_rules! lazy_regex {
    ($expr:expr) => {{
        static REGEX: ::once_cell::sync::Lazy<::regex::Regex> =
            ::once_cell::sync::Lazy::new(|| ::regex::Regex::new($expr).unwrap());
        &REGEX
    }};
    ($expr:expr,) => {
        lazy_regex!($expr)
    };
}

/// Lightly parse only the CSS that Calibre might emit, just enough so that
/// we know which selectors each block is for.
pub(crate) fn get_css_rulesets(css: &str) -> Vec<Ruleset> {
    // TODO: use a real parser, perhaps
    static RULESETS: &Lazy<Regex> = lazy_regex!(r"(?m)^(?P<selectors>[^{]+)\s*\{(?P<declaration_block>[^}]*)\}");
    RULESETS
        .captures_iter(css)
        .map(|m| Ruleset {
            selectors: m["selectors"].trim().to_string(),
            declaration_block: m["declaration_block"].trim().to_string(),
        }).collect()
}

pub(crate) fn get_all_font_stacks(css: &str) -> Vec<String> {
    let mut out = Vec::new();
    let rulesets = get_css_rulesets(css);
    for ruleset in rulesets {
        if ruleset.selectors == "@font-face" {
            continue;
        }
        static FONT_FAMILY: &Lazy<Regex> = lazy_regex!(r"(?m)^(?:\s*)font-family:\s*(?P<stack>[^;]+?);?$");
        for m in FONT_FAMILY.captures_iter(&ruleset.declaration_block) {
            out.push(m["stack"].to_string());
        }
    }
    out
}

pub(crate) fn top_css(
    fro: &FontReplacementOptions,
    max_width: &str,
    min_line_height: &str,
    inside_margin_when_wide: &str,
    inside_margin_when_narrow: &str,
    outside_bgcolor: &str,
    inside_bgcolor: &str,
) -> String {
    let FontReplacementOptions {
        min_font_size,
        base_font_size,
        base_font_family,
        monospace_font_family,
        ..
    } = fro;
    formatdoc!("
        /* unbook */

        :root {{
            --base-font-size: {base_font_size};
            --base-font-family: {base_font_family};
            --monospace-font-family: {monospace_font_family};
            --min-font-size: {min_font_size};
            --min-line-height: {min_line_height};
            --inside-margin-when-wide: {inside_margin_when_wide};
            --inside-margin-when-narrow: {inside_margin_when_narrow};
            --outside-bgcolor: {outside_bgcolor};
            --inside-bgcolor: {inside_bgcolor};
        }}

        html {{
            background-color: var(--outside-bgcolor);
        }}

        body {{
            background-color: var(--inside-bgcolor);
            max-width: {max_width};
            margin: 0 auto;
            padding: var(--inside-margin-when-narrow);

            line-height: var(--min-line-height);

            font-size: var(--base-font-size);
            /* Don't let iOS Safari enlarge the font size when the phone is in landscape mode.
             * https://kilianvalkhof.com/2022/css-html/your-css-reset-needs-text-size-adjust-probably/
             */
            -webkit-text-size-adjust: none;
            text-size-adjust: none;

            font-family: var(--base-font-family);

            /* Without word-break: break-word, iOS Safari 16.1 lets
             * very long words e.g. URLs widen the page */
            word-break: break-word;
        }}

        @media only screen and (min-width: calc({inside_margin_when_narrow} + {max_width} + {inside_margin_when_narrow})) {{
            body {{
                padding: var(--inside-margin-when-wide);
            }}
        }}

        sup, sub {{
            /* <sup> defaults to `vertical-align: super` styling, which causes the containing
             * line to be heightened in an ugly way. https://stackoverflow.com/a/6594576 says
             * to reduce the <sup>'s line-height, but setting 1.0 is not enough to fix it; 0
             * works but seems risky in case anyone has a multi-line <sup>; 0.9 may be enough
             * for most books, but is somewhat arbitrary and might similarly cause readability
             * problems. We use the fix from
             * https://css-tricks.com/snippets/css/prevent-superscripts-and-subscripts-from-affecting-line-height/
             * instead.
             */
            /* !important because books often put a vertical-align: 0.55em or similar on <sup> elements */
            vertical-align: baseline !important;
            position: relative;
            top: -0.4em;
        }}
        sub {{
            top: 0.4em;
        }}

        img {{
            /* We don't want to let images widen the page, especially on mobile.
             *
             * TODO: allow images to exceed the width of the container, but
             * not the viewport width, and without widening the viewport.
             * 
             * https://stackoverflow.com/a/41059954 does not work because it
             * widens the page when there is a wide image. */
            max-width: 100%;

            /* Some books have an explicit `width: ...px` and `height: ...px` on each image,
             * but we don't want the `height` to apply when we're max-width constrained because
             * that will incorrectly stretch the image.
             * 
             * TODO: only `auto` when we think the image isn't intentionally being made larger
             * or smaller. */
            height: auto !important;
            width: auto !important;

            /* Some books have images for e.g. mathematical formulas in the middle of a paragraph,
             * and we can make these look a little less terrible by vertical-aligning them to the
             * middle instead of the bottom. */
            vertical-align: middle;
        }}

        /* Center the cover when the image is smaller than the max-width */
        img.unbook-cover {{
            display: block;
            margin: 1em auto;
        }}

        /* calibre */
    ")
}

type GenericFamilyMap = HashMap<Option<GenericFontFamily>, HashSet<String>>;

pub(crate) fn get_generic_font_family_map(css: &str) -> GenericFamilyMap {
    let font_stacks = get_all_font_stacks(css);
    let mut family_map: HashMap<Option<GenericFontFamily>, HashSet<String>> = HashMap::with_capacity(6);
    for stack in font_stacks {
        let generic_family = classify_font_family(&stack);
        family_map.entry(generic_family).or_default().insert(stack);
    }
    family_map
}

fn make_combined_regex(items: &[&str]) -> String {
    let escaped_items: Vec<String> = items.iter().map(|item| regex::escape(item)).collect();
    let joined = escaped_items.join("|");
    let re = format!("({joined})");
    re
}

fn replace_font_stacks<'a>(css: &'a str, stacks: &[&str], replacement: &str) -> Cow<'a, str> {
    let re = make_combined_regex(stacks);
    let font_family = Regex::new(&format!(r"(?m)^(?P<indent>\s*)font-family:\s*(?P<stack>{re})\s*;?$")).unwrap();
    font_family.replace_all(css, &format!("${{indent}}font-family: {replacement}; /* was font-family: ${{stack}} */ /* unbook */"))
}

/// Fix just one declaration block (no selector)
pub(crate) fn fix_css_ruleset(
    ruleset: &Ruleset,
    fro: &FontReplacementOptions,
    family_map: &GenericFamilyMap,
    inside_bgcolor: Option<&Color>,
    inside_bgcolor_similarity_threshold: f64,
) -> Ruleset {
    let css = &ruleset.declaration_block;

    // Replace line-height overrides so that they are not smaller that our
    // minimum. A minimum line height aids in reading by reducing the chance
    // of regressing to an already-read line.
    static LINE_HEIGHT: &Lazy<Regex> = lazy_regex!(r"(?m)^(?P<indent>\s*)line-height:\s*(?P<height>[^;]+?);?$");
    let css = LINE_HEIGHT.replace_all(css, "${indent}line-height: max($height, var(--min-line-height)); /* unbook */");

    // Text that is too small either causes eye strain or becomes completely unreadable.
    static FONT_SIZE: &Lazy<Regex> = lazy_regex!(r"(?m)^(?P<indent>\s*)font-size:\s*(?P<size>[^;]+?);?$");
    let css = FONT_SIZE.replace_all(&css, "${indent}font-size: max($size, var(--min-font-size)); /* unbook */");

    // Justifying text to both the left and right edge creates uneven spacing
    // between words and impairs reading speed. It is also a lost cause on
    // mobile, where the width of the screen can be very narrow. For further
    // rationale and a demonstration, see _An Essay on Typography_,
    // Chapter 6 'The Procrustean Bed', pp. 88-93.
    // https://monoskop.org/images/8/8d/Gill_Eric_An_Essay_on_Typography.pdf#page=94
    static TEXT_ALIGN_JUSTIFY: &Lazy<Regex> = lazy_regex!(r"(?m)^(?P<indent>\s*)text-align:\s*justify;?$");
    let css = TEXT_ALIGN_JUSTIFY.replace_all(&css, "${indent}/* was text-align: justify; */ /* unbook */");

    // Some books have a margin-(top|bottom): 0.2em or similar on paragraphs, and
    // these paragraphs tend to have "para*" classes. Having small extra margins
    // between paragraphs is typographically incorrect and low risk to fix, because
    // e.g. 0.2em is close enough to 0 that we're unlikely to cause semantic damage.
    let selectors = &ruleset.selectors;
    let probably_a_paragraph =
        (selectors.starts_with(".calibre") && css.contains("text-indent:")) ||
        selectors == ".indent" ||
        selectors == ".noindent" ||
        selectors == ".indent-para" ||
        selectors.contains(".para") ||
        selectors.starts_with(".class_indent");
    let css = if probably_a_paragraph {
        static PARA_MARGIN_BOTTOM: &Lazy<Regex> = lazy_regex!(r"(?m)^(?P<indent>\s*)(?P<which>margin-(top|bottom)):\s*(?P<margin>0\.[123][\d]?em|[1234](\.\d+)?px|[1234](\.\d+)?pt);?$");
        let css = PARA_MARGIN_BOTTOM.replace_all(&css, "${indent}${which}: 0; /* was ${which}: ${margin}; */ /* unbook */");
        css
    } else {
        css
    };

    // Some books have a white or near-white background/background-color
    // that we want to get rid of, as we set our own background-color.
    let background_color_removal_candidate =
        // e.g. Time_to_Use_the_Modern_Digital_Publishing_Format.epub
        selectors == ".calibre" ||
        // e.g. pg6130-images.epub or anything else from Project Gutenberg
        selectors.starts_with(".x-ebookmaker");
    let css = if background_color_removal_candidate && inside_bgcolor.is_some() {
        let [our_r, our_g, our_b, _our_a] = inside_bgcolor.unwrap().to_array();
        static BACKGROUND_COLOR: &Lazy<Regex> = lazy_regex!(r"(?m)^(?P<indent>\s*)(?P<which>background(-color)?):\s*(?P<background_color>[^;]+?);?$");
        let css = BACKGROUND_COLOR.replace_all(&css, |caps: &Captures| {
            let indent = &caps["indent"];
            let which = &caps["which"];
            let background_color = &caps["background_color"];
            let Ok(parsed) = csscolorparser::parse(background_color) else {
                // Failed to parse; return the original color instead of modifying it
                return format!("{indent}{which}: {background_color};");
            };
            let [css_r, css_g, css_b, _css_a] = parsed.to_array();
            if
                (our_r - css_r).abs() > inside_bgcolor_similarity_threshold ||
                (our_g - css_g).abs() > inside_bgcolor_similarity_threshold ||
                (our_b - css_b).abs() > inside_bgcolor_similarity_threshold
            {
                // Too different; return the original color instead of modifying it
                return format!("{indent}{which}: {background_color};");
            }
            format!("{indent}{which}: inherit; /* was background-color: {background_color}; */ /* unbook */")
        });
        css.to_string()
    } else {
        css.to_string()
    };

    // Some books have <sup>-like citations except they're not a <sup> tag; detect
    // them by their `vertical-align: super` and apply the same fix we have for <sup>
    static VERTICAL_ALIGN_SUPER: &Lazy<Regex> = lazy_regex!(r"(?m)^(?P<indent>\s*)vertical-align:\s*super;?$");
    // We can't use ${indent} more than once (it's empty the second and third time?),
    // so just hardcode an indent :(
    let css = VERTICAL_ALIGN_SUPER.replace_all(&css, "\
        ${indent}vertical-align: baseline; /* was vertical-align: super; */ /* unbook */\n\
        \x20\x20\x20\x20position: relative; /* unbook */\n\
        \x20\x20\x20\x20top: -0.4em; /* unbook */");

    // Replace serif and sans-serif typefaces according to the user's preferences.
    // Authors and publishers sometimes want an ebook to use a certain typeface, but
    // the user's familiarity with their default sans-serif font (or other chosen
    // replacement) should override this, because it enables them to read faster.
    let css = match fro.replace_serif_and_sans_serif {
        FontFamilyReplacementMode::never => css,
        FontFamilyReplacementMode::if_one => {
            let empty = &HashSet::new();
            let serif = family_map.get(&Some(GenericFontFamily::Serif)).unwrap_or(empty);
            let sans_serif = family_map.get(&Some(GenericFontFamily::SansSerif)).unwrap_or(empty);
            let mut both: HashSet<&String> = serif.union(sans_serif).collect();
            if both.len() == 1 {
                let only = both.drain().next().unwrap();
                replace_font_stacks(&css, &[only], "var(--base-font-family)")
            } else {
                css
            }
        }
        FontFamilyReplacementMode::always => {
            let empty = &HashSet::new();
            let serif = family_map.get(&Some(GenericFontFamily::Serif)).unwrap_or(empty);
            let sans_serif = family_map.get(&Some(GenericFontFamily::SansSerif)).unwrap_or(empty);
            let mut both: HashSet<&String> = serif.union(sans_serif).collect();
            if !both.is_empty() {
                let stacks: Vec<&str> = both.drain().map(String::as_str).collect();
                replace_font_stacks(&css, &stacks, "var(--base-font-family)")
            } else {
                css
            }
        }
    };

    // Replace monospace font faces according to the user's preferences.
    let css = match fro.replace_monospace {
        FontFamilyReplacementMode::never => css,
        FontFamilyReplacementMode::if_one => {
            let empty = &HashSet::new();
            let mut monospace = family_map.get(&Some(GenericFontFamily::Monospace)).unwrap_or(empty).clone();
            if monospace.len() == 1 {
                let only = monospace.drain().next().unwrap();
                replace_font_stacks(&css, &[&only], "var(--monospace-font-family)")
            } else {
                css
            }
        }
        FontFamilyReplacementMode::always => {
            let empty = &HashSet::new();
            let monospace = family_map.get(&Some(GenericFontFamily::Monospace)).unwrap_or(empty);
            if !monospace.is_empty() {
                let stacks: Vec<&str> = monospace.iter().map(String::as_str).collect();
                replace_font_stacks(&css, &stacks, "var(--monospace-font-family)")
            } else {
                css
            }
        }
    };

    Ruleset { selectors: ruleset.selectors.clone(), declaration_block: css.to_string() }
}

pub(crate) fn fix_css(
    css: &str,
    fro: &FontReplacementOptions,
    family_map: &GenericFamilyMap,
    inside_bgcolor: &str,
    inside_bgcolor_similarity_threshold: f64,
) -> String {
    let mut out = String::with_capacity(css.len() + 4096);
    let inside_bgcolor: Option<Color> = csscolorparser::parse(inside_bgcolor).ok();

    let rulesets = get_css_rulesets(css);
    for ruleset in rulesets {
        if ruleset.selectors == "@font-face" {
            // Calibre currently doesn't include any OEBPS/fonts in HTMLZ output,
            // but we still include @font-face in the output to make the intended
            // font apparent.
            out.push_str(&ruleset.to_string());
        } else {
            let fixed_ruleset = fix_css_ruleset(&ruleset, fro, family_map, inside_bgcolor.as_ref(), inside_bgcolor_similarity_threshold);
            out.push_str(&fixed_ruleset.to_string());
        }
    }

    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn test_get_css_rulesets() {
        let css = indoc!("
            .block2, img {
                display: block;
                margin-bottom: 1em;
                }
            .block3 {
                color: red
            }
            .block4{
                color: blue
            }
            .block5{
                color: green
                }
        ");

        let expected = vec![
            Ruleset {
                selectors: ".block2, img".to_string(),
                declaration_block: "display: block;\n    margin-bottom: 1em;".to_string(),
            },
            Ruleset {
                selectors: ".block3".to_string(),
                declaration_block: "color: red".to_string(),
            },
            Ruleset {
                selectors: ".block4".to_string(),
                declaration_block: "color: blue".to_string(),
            },
            Ruleset {
                selectors: ".block5".to_string(),
                declaration_block: "color: green".to_string(),
            },
        ];

        assert_eq!(get_css_rulesets(css), expected);
    }

    #[test]
    fn test_get_all_font_stacks() {
        // Any @font-face should be ignored
        let input = "
            @font-face {
                font-family: Something;
                font-style: normal;
                font-weight: normal;
                src: url(OEBPS/fonts/Something.ttf)
            }

            .something {
                font-family: Verdana, sans-serif
                font-family:Verdana;
                font-size: 20px;
            }
    
            .something-else {
            font-family: system-ui;
            font-family: Arial;
            }
        ";
    
        let expected = vec![
            "Verdana, sans-serif",
            "Verdana",
            "system-ui",
            "Arial",
        ];
    
        assert_eq!(get_all_font_stacks(input), expected);
    }    

    fn dummy_fro() -> FontReplacementOptions {
        FontReplacementOptions {
            min_font_size: "".to_string(),
            base_font_size: "".to_string(),
            base_font_family: "".to_string(),
            monospace_font_family: "".to_string(),
            replace_serif_and_sans_serif: FontFamilyReplacementMode::never,
            replace_monospace: FontFamilyReplacementMode::never,
        }
    }

    #[test]
    fn test_fix_css_line_height() {
        let input = indoc!("
            .something {
                line-height: 1.2
            }
            .something-else {
                line-height: 1.3;
                font-family: Arial
            }
        ");

        let output = indoc!("
            .something {
                line-height: max(1.2, var(--min-line-height)); /* unbook */
            }
            .something-else {
                line-height: max(1.3, var(--min-line-height)); /* unbook */
                font-family: Arial
            }
        ");

        assert_eq!(fix_css(input, &dummy_fro(), &get_generic_font_family_map(input), "#e9e9e9", 0.2), output);
    }

    #[test]
    fn test_fix_css_text_align() {
        let input = indoc!("
            .something-1 {
                text-align: right;
                text-align: right
            }
            .something-2 {
                text-align: left;
                text-align: left
            }
            .something-3 {
                text-align: justify
            }
            .something-4 {
                text-align: justify;
            }
        ");

        let output = indoc!("
            .something-1 {
                text-align: right;
                text-align: right
            }
            .something-2 {
                text-align: left;
                text-align: left
            }
            .something-3 {
                /* was text-align: justify; */ /* unbook */
            }
            .something-4 {
                /* was text-align: justify; */ /* unbook */
            }
        ");

        assert_eq!(fix_css(input, &dummy_fro(), &get_generic_font_family_map(input), "#e9e9e9", 0.2), output);
    }

    #[test]
    fn test_fix_font_size() {
        let input = indoc!("
            .something {
                font-size: 12px
            }
            .something-else {
                font-size: 14pt;
            }
        ");

        let output = indoc!("
            .something {
                font-size: max(12px, var(--min-font-size)); /* unbook */
            }
            .something-else {
                font-size: max(14pt, var(--min-font-size)); /* unbook */
            }
        ");

        assert_eq!(fix_css(input, &dummy_fro(), &get_generic_font_family_map(input), "#e9e9e9", 0.2), output);
    }

    #[test]
    fn test_fix_para_margin_bottom() {
        let input = indoc!("
            .para-p1 {
                margin-bottom: 0.5em
            }
            .para-p2 {
                margin-bottom: 0.2em
            }
            .para-p3 {
                margin-bottom: 0.23em
            }
            .something {
                margin-bottom: 0.2em;
            }
            .indent {
                margin-bottom: 0.1em
            }
            .noindent {
                margin-top: 1pt
            }
            .para {
                margin-top: 1px;
                margin-bottom: 1px;
                margin-top: 2px;
                margin-bottom: 2px;
                margin-top: 3px;
                margin-bottom: 3px;
                margin-top: 4.99px;
                margin-bottom: 4.99px;
                margin-top: 5px;
                margin-bottom: 5px;
            }
            .class_indent1 {
                margin-top: 0.2em;
            }
        ");

        let output = indoc!("
            .para-p1 {
                margin-bottom: 0.5em
            }
            .para-p2 {
                margin-bottom: 0; /* was margin-bottom: 0.2em; */ /* unbook */
            }
            .para-p3 {
                margin-bottom: 0; /* was margin-bottom: 0.23em; */ /* unbook */
            }
            .something {
                margin-bottom: 0.2em;
            }
            .indent {
                margin-bottom: 0; /* was margin-bottom: 0.1em; */ /* unbook */
            }
            .noindent {
                margin-top: 0; /* was margin-top: 1pt; */ /* unbook */
            }
            .para {
                margin-top: 0; /* was margin-top: 1px; */ /* unbook */
                margin-bottom: 0; /* was margin-bottom: 1px; */ /* unbook */
                margin-top: 0; /* was margin-top: 2px; */ /* unbook */
                margin-bottom: 0; /* was margin-bottom: 2px; */ /* unbook */
                margin-top: 0; /* was margin-top: 3px; */ /* unbook */
                margin-bottom: 0; /* was margin-bottom: 3px; */ /* unbook */
                margin-top: 0; /* was margin-top: 4.99px; */ /* unbook */
                margin-bottom: 0; /* was margin-bottom: 4.99px; */ /* unbook */
                margin-top: 5px;
                margin-bottom: 5px;
            }
            .class_indent1 {
                margin-top: 0; /* was margin-top: 0.2em; */ /* unbook */
            }
        ");

        assert_eq!(fix_css(input, &dummy_fro(), &get_generic_font_family_map(input), "#e9e9e9", 0.2), output);
    }

    #[test]
    fn test_fix_sup_like() {
        let input = indoc!("
            .calibre8 {
                vertical-align: super;
            }
            .someting {
                vertical-align: top;
            }
        ");

        let output = indoc!("
            .calibre8 {
                vertical-align: baseline; /* was vertical-align: super; */ /* unbook */
                position: relative; /* unbook */
                top: -0.4em; /* unbook */
            }
            .someting {
                vertical-align: top;
            }
        ");

        assert_eq!(fix_css(input, &dummy_fro(), &get_generic_font_family_map(input), "#e9e9e9", 0.2), output);
    }

    fn input_with_one_font_family() -> &'static str {
        // Any @font-face should be ignored
        indoc!("
            @font-face {
                font-family: Arial;
                font-style: normal;
                font-weight: normal;
                src: url(OEBPS/fonts/Arial.ttf)
            }
            .something {
                font-family: Verdana, sans-serif
            }
            .something-else {
                font-family: Verdana, sans-serif;
            }
            pre {
                font-family: Courier, monospace
            }
            code {
                font-family: Courier, monospace;
            }
        ")
    }

    fn input_with_distinct_font_families() -> &'static str {
        indoc!("
            .something {
                font-family: Verdana, sans-serif
            }
            .something-else {
                font-family: Times, serif;
            }
            pre {
                font-family: Courier, monospace
            }
            code {
                font-family: Consolas, monospace;
            }
        ")
    }

    #[test]
    fn test_fix_font_family_never() {
        let input = input_with_one_font_family();
        assert_eq!(fix_css(input, &dummy_fro(), &get_generic_font_family_map(input), "#e9e9e9", 0.2), input);
    }

    #[test]
    fn test_fix_font_family_if_one_base() {
        let output = indoc!("
            @font-face {
                font-family: Arial;
                font-style: normal;
                font-weight: normal;
                src: url(OEBPS/fonts/Arial.ttf)
            }
            .something {
                font-family: var(--base-font-family); /* was font-family: Verdana, sans-serif */ /* unbook */
            }
            .something-else {
                font-family: var(--base-font-family); /* was font-family: Verdana, sans-serif */ /* unbook */
            }
            pre {
                font-family: Courier, monospace
            }
            code {
                font-family: Courier, monospace;
            }
        ");

        let input = input_with_one_font_family();
        let mut fro = dummy_fro();
        for mode in [FontFamilyReplacementMode::if_one, FontFamilyReplacementMode::always] {
            fro.replace_serif_and_sans_serif = mode;
            assert_eq!(fix_css(input, &fro, &get_generic_font_family_map(input), "#e9e9e9", 0.2), output);
        }
    }

    #[test]
    fn test_fix_font_family_if_one_base_distinct() {
        let input = input_with_distinct_font_families();
        let mut fro = dummy_fro();
        fro.replace_serif_and_sans_serif = FontFamilyReplacementMode::if_one;
        fro.replace_monospace = FontFamilyReplacementMode::if_one;
        assert_eq!(fix_css(input, &fro, &get_generic_font_family_map(input), "#e9e9e9", 0.2), input);
    }

    #[test]
    fn test_fix_font_family_both() {
        let output = indoc!("
            @font-face {
                font-family: Arial;
                font-style: normal;
                font-weight: normal;
                src: url(OEBPS/fonts/Arial.ttf)
            }
            .something {
                font-family: var(--base-font-family); /* was font-family: Verdana, sans-serif */ /* unbook */
            }
            .something-else {
                font-family: var(--base-font-family); /* was font-family: Verdana, sans-serif */ /* unbook */
            }
            pre {
                font-family: var(--monospace-font-family); /* was font-family: Courier, monospace */ /* unbook */
            }
            code {
                font-family: var(--monospace-font-family); /* was font-family: Courier, monospace */ /* unbook */
            }
        ");

        let input = input_with_one_font_family();
        let mut fro = dummy_fro();
        for mode in [FontFamilyReplacementMode::if_one, FontFamilyReplacementMode::always] {
            fro.replace_serif_and_sans_serif = mode;
            fro.replace_monospace = mode;
            assert_eq!(fix_css(input, &fro, &get_generic_font_family_map(input), "#e9e9e9", 0.2), output);
        }
    }

    #[test]
    fn test_fix_font_family_always() {
        let output = indoc!("
            .something {
                font-family: var(--base-font-family); /* was font-family: Verdana, sans-serif */ /* unbook */
            }
            .something-else {
                font-family: var(--base-font-family); /* was font-family: Times, serif */ /* unbook */
            }
            pre {
                font-family: var(--monospace-font-family); /* was font-family: Courier, monospace */ /* unbook */
            }
            code {
                font-family: var(--monospace-font-family); /* was font-family: Consolas, monospace */ /* unbook */
            }
        ");

        let input = input_with_distinct_font_families();
        let mut fro = dummy_fro();
        fro.replace_serif_and_sans_serif = FontFamilyReplacementMode::always;
        fro.replace_monospace = FontFamilyReplacementMode::always;
        assert_eq!(fix_css(input, &fro, &get_generic_font_family_map(input), "#e9e9e9", 0.2), output);
    }

    #[test]
    fn test_fix_background_color() {
        let input = indoc!("
            .something {
                background-color: #fff;
                background-color: black;
            }
            .calibre {
                background-color: #000;
                background-color: black;
                background-color: #fff;
                background-color: #eee;
                background-color: #ffffff;
                background-color: white;
                background-color: rgb(255, 255, 255);
            }
            .x-ebookmaker {
                background-color: #000;
                background-color: black;
                background-color: #fff;
                background-color: #eee;
                background-color: #ffffff;
                background-color: white;
                background-color: rgb(255, 255, 255);
            }
        ");

        let output = indoc!("
            .something {
                background-color: #fff;
                background-color: black;
            }
            .calibre {
                background-color: #000;
                background-color: black;
                background-color: inherit; /* was background-color: #fff; */ /* unbook */
                background-color: inherit; /* was background-color: #eee; */ /* unbook */
                background-color: inherit; /* was background-color: #ffffff; */ /* unbook */
                background-color: inherit; /* was background-color: white; */ /* unbook */
                background-color: inherit; /* was background-color: rgb(255, 255, 255); */ /* unbook */
            }
            .x-ebookmaker {
                background-color: #000;
                background-color: black;
                background-color: inherit; /* was background-color: #fff; */ /* unbook */
                background-color: inherit; /* was background-color: #eee; */ /* unbook */
                background-color: inherit; /* was background-color: #ffffff; */ /* unbook */
                background-color: inherit; /* was background-color: white; */ /* unbook */
                background-color: inherit; /* was background-color: rgb(255, 255, 255); */ /* unbook */
            }
        ");

        let fro = dummy_fro();
        assert_eq!(fix_css(input, &fro, &get_generic_font_family_map(input), "#e9e9e9", 0.2), output);
    }
}
