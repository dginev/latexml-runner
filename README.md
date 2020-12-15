# latexml-rust
Rust runner for high-performance conversions with latexmls.

This executable is useful over the native Perl executables of latexml if:
 - You have a large number of tasks with a shared set of TeX/LaTeX packages and preambles.
 - The tasks are small enough that latexml's overhead is a performance bottleneck (e.g. single formulas or abstracts/paragraphs).
 - Your system installation allows for a custom perl plugin
 - concretely [latexmls](https://github.com/dginev/LaTeXML-Plugin-latexmls), a socket server for LaTeXML.

If your conversion task requires a distributed setup and/or is unable to preload most dependencies, this crate won't offer you much of a speedup. For such cases, consider [LaTeXML::Plugin::Cortex](https://github.com/dginev/LaTeXML-Plugin-cortex) as one alternative.
