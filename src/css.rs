use indoc::formatdoc;
use regex::Regex;

pub(crate) fn get_all_font_family(css: &str) -> Vec<String> {
    let font_family = Regex::new(r"(?m)^(?:\s*)font-family:\s*(?P<stack>[^;]+?);?$").unwrap();
    font_family.captures_iter(css).map(|m| m["stack"].to_string()).collect()
}

pub(crate) fn top_css(base_font_size: &str, base_font_family: &str, monospace_font_family: &str, min_font_size: &str, max_width: &str, min_line_height: &str) -> String {
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
            padding: 1em;

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

        /* calibre */
    ")
}

pub(crate) fn fix_css(css: &str) -> String {
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
    let css = text_align_justify.replace_all(&css, "${indent}/* text-align: justify; */ /* unbook */");

    css.to_string()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn test_get_all_font_family() {
        let input = "
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

        assert_eq!(get_all_font_family(input), expected);
    }

    #[test]
    fn test_fix_css_line_height() {
        let input = "
            .something {
                line-height: 1.2
            }

            .something-else {
                line-height: 1.3;
                font-family: Arial
            }
        ";

        let output = "
            .something {
                line-height: max(1.2, var(--min-line-height)); /* unbook */
            }

            .something-else {
                line-height: max(1.3, var(--min-line-height)); /* unbook */
                font-family: Arial
            }
        ";

        assert_eq!(fix_css(input), output);
    }

    #[test]
    fn test_fix_css_text_align() {
        let input = "
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
        ";

        let output = "
            .something-1 {
                text-align: right;
                text-align: right
            }

            .something-2 {
                text-align: left;
                text-align: left
            }

            .something-3 {
                /* text-align: justify; */ /* unbook */
            }

            .something-4 {
                /* text-align: justify; */ /* unbook */
            }
        ";

        assert_eq!(fix_css(input), output);
    }

    #[test]
    fn test_fix_font_size() {
        let input = "
            .something {
                font-size: 12px
            }

            .something-else {
                font-size: 14pt;
            }
        ";

        let output = "
            .something {
                font-size: max(12px, var(--min-font-size)); /* unbook */
            }

            .something-else {
                font-size: max(14pt, var(--min-font-size)); /* unbook */
            }
        ";

        assert_eq!(fix_css(input), output);
    }
}
