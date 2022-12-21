use std::{collections::{HashMap, HashSet}, borrow::Cow};

use clap::ValueEnum;
use indoc::formatdoc;
use regex::Regex;
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

/// Lightly parse only the CSS that Calibre might emit, just enough so that
/// we know which selectors each block is for.
pub(crate) fn get_css_rulesets(css: &str) -> Vec<Ruleset> {
    // TODO: use a real parser, perhaps
    let rulesets = Regex::new(r"(?m)^(?P<selectors>[^{]+)\s*\{(?P<declaration_block>[^}]*)\}").unwrap();
    rulesets
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
        let font_family = Regex::new(r"(?m)^(?:\s*)font-family:\s*(?P<stack>[^;]+?);?$").unwrap();
        for m in font_family.captures_iter(&ruleset.declaration_block) {
            out.push(m["stack"].to_string());
        }
    }
    out
}

pub(crate) fn top_css(fro: &FontReplacementOptions, max_width: &str, min_line_height: &str) -> String {
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
        }}

        body {{
            max-width: {max_width};
            margin: 0 auto;
            padding: 16px;

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
            vertical-align: baseline;
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
        family_map.entry(generic_family).or_insert_with(HashSet::new).insert(stack);
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
pub(crate) fn fix_css_ruleset(ruleset: &Ruleset, fro: &FontReplacementOptions, family_map: &GenericFamilyMap) -> Ruleset {
    let css = &ruleset.declaration_block;

    // Replace line-height overrides so that they are not smaller that our
    // minimum. A minimum line height aids in reading by reducing the chance
    // of regressing to an already-read line.
    let line_height = Regex::new(r"(?m)^(?P<indent>\s*)line-height:\s*(?P<height>[^;]+?);?$").unwrap();
    let css = line_height.replace_all(css, "${indent}line-height: max($height, var(--min-line-height)); /* unbook */");

    // Text that is too small either causes eye strain or becomes completely unreadable.
    let font_size = Regex::new(r"(?m)^(?P<indent>\s*)font-size:\s*(?P<size>[^;]+?);?$").unwrap();
    let css = font_size.replace_all(&css, "${indent}font-size: max($size, var(--min-font-size)); /* unbook */");

    // Justifying text to both the left and right edge creates uneven spacing
    // between words and impairs reading speed. It is also a lost cause on
    // mobile, where the width of the screen can be very narrow. For further
    // rationale and a demonstration, see _An Essay on Typography_,
    // Chapter 6 'The Procrustean Bed', pp. 88-93.
    // https://monoskop.org/images/8/8d/Gill_Eric_An_Essay_on_Typography.pdf#page=94
    let text_align_justify = Regex::new(r"(?m)^(?P<indent>\s*)text-align:\s*justify;?$").unwrap();
    let css = text_align_justify.replace_all(&css, "${indent}/* was text-align: justify; */ /* unbook */");

    // Some books have a margin-bottom: 0.2em on paragraphs, and these paragraphs
    // tend to have "para-" classes. Having small extra margins between paragraphs
    // is typographically incorrect and low risk to fix, because 0.2em is close
    // enough to 0 that we're unlikely to cause semantic damage.
    let css = if ruleset.selectors.contains(".para-") {
        let para_margin_bottom = Regex::new(r"(?m)^(?P<indent>\s*)margin-bottom:\s*(?P<margin_bottom>0\.2em);?$").unwrap();
        let css = para_margin_bottom.replace_all(&css, "${indent}margin-bottom: 0; /* was margin-bottom: ${margin_bottom}; */ /* unbook */");
        css
    } else {
        css
    };

    // Some books have <sup>-like citations except they're not a <sup> tag; detect
    // them by their `vertical-align: super` and apply the same fix we have for <sup>
    let vertical_align_super = Regex::new(r"(?m)^(?P<indent>\s*)vertical-align:\s*super;?$").unwrap();
    // We can't use ${indent} more than once (it's empty the second and third time?),
    // we just hardcode an indent :(
    let css = vertical_align_super.replace_all(&css, "\
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

pub(crate) fn fix_css(css: &str, fro: &FontReplacementOptions, family_map: &GenericFamilyMap) -> String {
    let mut out = String::with_capacity(css.len() + 4096);

    let rulesets = get_css_rulesets(css);
    for ruleset in rulesets {
        if ruleset.selectors == "@font-face" {
            // Calibre currently doesn't include any OEBPS/fonts in HTMLZ output,
            // but we still include @font-face in the output to make the intended
            // font apparent.
            out.push_str(&ruleset.to_string());
        } else {
            let fixed_ruleset = fix_css_ruleset(&ruleset, fro, family_map);
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

        assert_eq!(fix_css(input, &dummy_fro(), &get_generic_font_family_map(input)), output);
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

        assert_eq!(fix_css(input, &dummy_fro(), &get_generic_font_family_map(input)), output);
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

        assert_eq!(fix_css(input, &dummy_fro(), &get_generic_font_family_map(input)), output);
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
            .something {
                margin-bottom: 0.2em;
            }
        ");

        let output = indoc!("
            .para-p1 {
                margin-bottom: 0.5em
            }
            .para-p2 {
                margin-bottom: 0; /* was margin-bottom: 0.2em; */ /* unbook */
            }
            .something {
                margin-bottom: 0.2em;
            }
        ");

        assert_eq!(fix_css(input, &dummy_fro(), &get_generic_font_family_map(input)), output);
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

        assert_eq!(fix_css(input, &dummy_fro(), &get_generic_font_family_map(input)), output);
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
        assert_eq!(fix_css(input, &dummy_fro(), &get_generic_font_family_map(input)), input);
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
            assert_eq!(fix_css(input, &fro, &get_generic_font_family_map(input)), output);
        }
    }

    #[test]
    fn test_fix_font_family_if_one_base_distinct() {
        let input = input_with_distinct_font_families();
        let mut fro = dummy_fro();
        fro.replace_serif_and_sans_serif = FontFamilyReplacementMode::if_one;
        fro.replace_monospace = FontFamilyReplacementMode::if_one;
        assert_eq!(fix_css(input, &fro, &get_generic_font_family_map(input)), input);
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
            assert_eq!(fix_css(input, &fro, &get_generic_font_family_map(input)), output);
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
        assert_eq!(fix_css(input, &fro, &get_generic_font_family_map(input)), output);
    }
}
