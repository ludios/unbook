use std::collections::HashMap;
use lazy_static::lazy_static;

fn parse_font_family_list(value: &str) -> Vec<String> {
    let value = value.trim();
    if value.is_empty() {
        return vec![];
    }
    let list = value.split(',');
    let trim: &[_] = &[' ', '\t', ',', '\'', '"'];
    list.map(|f| f.trim_matches(trim).to_string()).collect()
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum GenericFontFamily {
    Serif,
    SansSerif,
    Monospace,
    Cursive,
    Fantasy,
}

lazy_static! {
    // Based on https://www.w3.org/Style/Examples/007/fonts.en.html
    // but with additional fonts added.
    //
    // https://developer.mozilla.org/en-US/docs/Web/CSS/font-family
    // https://en.wikipedia.org/wiki/List_of_typefaces_included_with_Microsoft_Windows

    static ref LOWER_FACE_TO_GENERIC_FAMILY: HashMap<String, GenericFontFamily> = {    
        let serif = vec![
            "Times",
            "TimesBold",
            "TimesBoldItalic",
            "TimesItalic",
            "Timesb",
            "Timesbd",
            "Timesi",
            "Times New Roman",
            "Times New RomanB",
            "Times New RomanBI",
            "Times New RomanI",
            "TimesNewRomanPSMT",
            "Antiqua",
            "ANTQUAB",
            "ANTQUABI",
            "ANTQUAI",
            "Didot",
            "Georgia",
            "Palatino",
            "Palatino Linotype",
            "Garamond",
            "Adobe Garamond Pro",
            "URW Palladio L",
            "Bookman",
            "URW Bookman L",
            "New Century Schoolbook",
            "TeX Gyre Schola",
            "American Typewriter",
            "Charis",
            "CharisSILR",
            "CharisSILB",
            "Bitstream Vera Serif",
            "DejaVuSerif",
            "Shift",
            "Shift Light",
            "Alegreya",
            "Genr102", // Gentium
            "Geni102", // Gentium
            "Sylfaen",
            "Bodoni LT Pro",
            "Constantia",
            "Constantia Italic",
            "Adobe Caslon Pro",
            "Minion Pro",
            "Minion Pro Cond",
            "Kozuka Mincho Pr6N",
            "Kozuka Mincho Pr6N L",
            "Kozuka Mincho Pr6N R",
            "Adobe Song Std",
            "AdobeSongStd-Light",
            "VeljovicStd",
            "ITC Fenice Std",
            "Stempel Garamond LT Std",
            "STKai",
            "Traveling _Typewriter", // Not monospace: http://fontzzz.com/font/3731_traveling_typewriter.htm
            "serif",
            "ui-serif",
        ];

        let sans_serif = vec![
            "Arial",
            "Arial Unicode MS",
            "ARIALUNI",
            "Helvetica",
            "HelveticaNeueLTStd",
            "HelveticaNeueLTStd-BdCn",
            "HelveticaNeueLTStd-BdCnO",
            "HelveticaNeueLTStd-Cn",
            "HelveticaNeueLTStd-Md",
            "HelveticaNeueLTStd-MdCn",
            "HelveticaNeueLTStd-MdCnO",
            "Verdana",
            "Trebuchet MS",
            "Calibri",
            "CALIBRIB",
            "CALIBRII",
            "Gill Sans",
            "Noto Sans",
            "Avantgarde",
            "DejaVu Sans",
            "DejaVuSans",
            "Bitstream Vera Sans",
            "TeX Gyre Adventor",
            "URW Gothic L",
            "Optima",
            "Gotham",
            "Arial Narrow",
            "Roboto",
            "Inter",
            "PT Sans",
            "Open Sans",
            "Segoe UI",
            "Geneva",
            "Candara",
            "Franklin",
            "Franklin Medium",
            "Futura",
            "Futura Bold",
            "Futura Std Book",
            "DIN Next LT Pro",
            "Trade Gothic Next LT Pro",
            "Myriad",
            "Myriad Pro",
            "MyriadPro-Regular",
            "MyriadPro-Bold",
            "MyriadPro-BoldIt",
            "MyriadPro-It",
            "Quicksand",
            "Alegreya Sans",
            "Fort-Book",
            "Free Sans",
            "Free Sans Bold",
            "Liberation",
            "LiberationNarrow",
            "ＭＳ Ｐゴシック",
            "KaiTi",
            "SimHei",
            "AkzidenzStd",
            "ITCAvantGardeStd",
            "TradeGothicLTStd18",
            "TradeGothicLTStd20",
            "sans-serif",
            "ui-sans-serif",
            "system-ui",
            "-apple-system",
            "BlinkMacSystemFont",
        ];

        let monospace = vec![
            "Andale Mono",
            "Courier",
            "Courier New",
            "Courier New Bold",
            "Courier New Bold Italic",
            "Courier New Italic",
            "FreeMono",
            "OCR A Std",
            "DejaVu Sans Mono",
            "Consolas",
            "Lucida Console",
            "UbuntuMono",
            "Ubuntu Mono Bold",
            "Ubuntu Mono BoldItal",
            "Ubuntu Mono Ital",
            "monospace",
            "ui-monospace",
        ];
    
        let cursive = vec![
            "Comic Sans MS",
            "Comic Sans",
            "Apple Chancery",
            "Bradley Hand",
            "Brush Script MT",
            "Brush Script Std",
            "Snell Roundhand",
            "URW Chancery L",
            "Great Vibes",
            "cursive",
        ];
    
        let fantasy = vec![
            "Impact",
            "Luminari",
            "Chalkduster",
            "Jazz LET",
            "Blippo",
            "Stencil Std",
            "Marker Felt",
            "Trattatello",
            "fantasy",
        ];

        let mut map = HashMap::new();

        for (faces, generic) in [
            (serif, GenericFontFamily::Serif),
            (sans_serif, GenericFontFamily::SansSerif),
            (monospace, GenericFontFamily::Monospace),
            (fantasy, GenericFontFamily::Fantasy),
            (cursive, GenericFontFamily::Cursive),
        ].into_iter() {
            for face in faces {
                map.insert(face.to_lowercase(), generic);
            }
        }
        map
    };
}

// Books don't always have a generic font family at the end of a `font-family` list,
// so we need to be able to classify all the web safe fonts.
pub(crate) fn classify_font_family(css_value: &str) -> Option<GenericFontFamily> {
    let fonts = parse_font_family_list(&css_value.to_lowercase());
    for font in fonts {
        if let Some(generic) = LOWER_FACE_TO_GENERIC_FAMILY.get(&font) {
            return Some(*generic);
        }
    }
    None
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn test_parse_font_family_list() {
        let empty: Vec<String> = vec![];
        assert_eq!(parse_font_family_list(""), empty);
        assert_eq!(parse_font_family_list(" "), empty);
        assert_eq!(parse_font_family_list("\t \t"), empty);
        assert_eq!(parse_font_family_list("sans-serif"), vec!["sans-serif"]);
        assert_eq!(parse_font_family_list(" sans-serif "), vec!["sans-serif"]);
        assert_eq!(parse_font_family_list("A ,  With Spaces,'Single-quoted thing',  \"Double-quoted thing\" "),
            vec!["A", "With Spaces", "Single-quoted thing", "Double-quoted thing"]);
    }

    #[test]
    fn test_classify_font_family() {
        assert_eq!(classify_font_family(""), None);
        assert_eq!(classify_font_family("unknown"), None);
        assert_eq!(classify_font_family("sans-serif"), Some(GenericFontFamily::SansSerif));
        assert_eq!(classify_font_family("arial, serif, fantasy"), Some(GenericFontFamily::SansSerif));
        assert_eq!(classify_font_family("Arial, serif, cursive"), Some(GenericFontFamily::SansSerif));
        assert_eq!(classify_font_family("ARIAL, serif, serif"), Some(GenericFontFamily::SansSerif));
        assert_eq!(classify_font_family("Times, ARIAL, serif, serif"), Some(GenericFontFamily::Serif));
        assert_eq!(classify_font_family("\"Times New Roman\", ARIAL, serif, serif"), Some(GenericFontFamily::Serif));
        assert_eq!(classify_font_family("courier, ARIAL, serif, serif"), Some(GenericFontFamily::Monospace));
        assert_eq!(classify_font_family("Blippo, serif"), Some(GenericFontFamily::Fantasy));
        assert_eq!(classify_font_family("'Comic Sans', serif"), Some(GenericFontFamily::Cursive));
    }
}
