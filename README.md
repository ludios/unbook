[WIP, send feedback but do not announce it for me]

# unbook

unbook is a command-line program for converting a DRM-free .epub, .mobi, .azw, .azw3, .lit, or .chm ebook to a self-contained HTML file. PDF is **not** supported. In the HTML output, all images are included inline as base64, inspired by [SingleFile](https://github.com/gildas-lormeau/SingleFile). unbook adds some CSS to render things nicely on both large screens and mobile. You can open unbook's output HTML in any browser, experience normal scrolling behavior, and read with whatever browser extensions and bookmarklets you like.

<!--Sample output (processing [this input file]). Compare with [Calibre's HTMLZ output] (which unbook uses and postprocesses).-->

To use unbook: install Calibre, install a stable Rust compiler via rustup, then:

```bash
cargo install --git https://github.com/ludios/unbook
```

Usage:

```bash
unbook PATH_TO_EBOOK # write .html file to the same directory
unbook PATH_TO_EBOOK -o out.html
unbook -f PATH_TO_EBOOK # replace .html file if already exists
unbook --replace-serif-and-sans-serif always # replace typeface even when the book uses several
unbook --help
```

## Use cases

*	Read entire books in your browser because you like it or because it provides functionality not available in e-readers or ebook software
	*	e.g. bookmarklets, extensions like <a href="https://github.com/birchill/10ten-ja-reader#readme">10ten Japanese Reader</a>
*	Skim or search many ebooks using your browser
*	Share a book with someone who has a browser but no e-reader or ebook software
*	Link someone to a passage in a book using your browser's "Copy link to highlight" feature
*	Text-index books with software that supports HTML but not ebook formats

## Limitations

*	unbook produces a long HTML file without a chapter browser or fancy reader features. It does not save your reading position, though your browser may succeed at this if you never click an #anchor. unbook does not provide text adjustments (instead, re-run unbook with the settings you like).

*	Some ebooks, mostly those with large images, become too large when converted to a single HTML file. These may just be unsuitable for conversion using unbook.

*	unbook does not generate "dark mode" CSS because there is no way to generate an authoritative "dark" version of a book without manual review: consider photos and diagrams; some images need to be inverted while others do not. Some books have more complicated use of color in tables and SVG.

    Please use <a href="https://darkreader.org/">Dark Reader</a> if you need dark mode. On mobile, it is <a href="https://darkreader.org/blog/mobile/">available on iOS for Safari, and on Android for Firefox and Kiwi Browser</a>.

    Dark Reader does not generally invert images. If most of the images in a book should be inverted, use <a href="https://github.com/ludios/useful-bookmarklets#invert-all-images">this 'Invert all images' bookmarklet</a>.

*	Embedded fonts are currently lost due to a Calibre limitation.

*   `background-image`s in EPUB3 files are currently lost <a href="https://bugs.launchpad.net/calibre/+bug/1999956">due to a Calibre limitation</a>.

## Alternatives which don't quite solve the same problem

*   Convert to .epub if necessary and extract as a ZIP
	*   You'll get one XHTML file per chapter.
*   <a href="https://manual.calibre-ebook.com/server.html">The calibre Content server</a>


## `--help`

```
unbook --help
Convert an ebook to a single HTML file

Usage: unbook [OPTIONS] <EBOOK_PATH>

Arguments:
  <EBOOK_PATH>
          The path to an .{epub,mobi,azw,azw3,lit} file, or other format that Calibre can reasonably convert to HTMLZ. See https://manual.calibre-ebook.com/faq.html for a list of formats it supports, not all of which will convert nicely to HTMLZ

Options:
  -o, --output-path <OUTPUT_PATH>
          The path for the output .html file. If not specified, it is saved in the directory of the input file, with ".html" appended to the existing extension

  -e, --remove-ebook-ext
          Whether to remove the ebook extension before appending ".html".

          This is not the default because it makes it harder to find the original ebook file when viewing the .html, and because you may have e.g. both .mobi and .epub with the same name in a directory.

  -f, --force
          Whether to replace the output .html file if it already exists

      --base-font-size <BASE_FONT_SIZE>
          The base font-size (with a CSS unit) to use for the book text

          [default: 15px]

      --base-font-family <BASE_FONT_FAMILY>
          The base font-family to use for the book text

          [default: sans-serif]

      --monospace-font-family <MONOSPACE_FONT_FAMILY>
          The monospace font-family to use

          [default: monospace]

      --replace-serif-and-sans-serif <REPLACE_SERIF_AND_SANS_SERIF>
          Whether to replace `font-family` for all font stacks indicating serif or sans-serif fonts, with the base font family. The default "if-one" does this only when there is just one distinct font stack. This performs the font replacement only when there is no chance that distinct fonts are used to indicate something in the book

          [default: if-one]
          [possible values: never, if-one, always]

      --replace-monospace <REPLACE_MONOSPACE>
          Whether to replace `font-family` for all font stacks indicating monospace fonts, with the monospace font family. The default "if-one" does this only when there is just one distinct font stack

          [default: if-one]
          [possible values: never, if-one, always]

      --min-font-size <MIN_FONT_SIZE>
          The minimum font-size (with a CSS unit) to use for the book text. This can be used to work around issues with bad 'em' sizing making fonts far too small

          [default: 13px]

      --max-width <MAX_WIDTH>
          The max-width (with a CSS unit) to use for the book text

          [default: 5.5in]

      --min-line-height <MIN_LINE_HEIGHT>
          The minimum line-height (with an optional CSS unit) to use for the book text

          [default: 1.5]

      --ebook-convert <EBOOK_CONVERT>
          Path to the Calibre "ebook-convert" executable to use

          [default: ebook-convert]

      --keep-temporary-htmlz
          Whether to keep the temporary HTMLZ for debugging purposes

      --text-fragments-polyfill <TEXT_FRAGMENTS_POLYFILL>
          Which type of Text Fragments polyfill to add (if any) for the benefit of Firefox and Safari < 16.1 users

          [default: inline]
          [possible values: none, inline, unpkg]

  -h, --help
          Print help information (use `-h` for a summary)

  -V, --version
          Print version information
```
