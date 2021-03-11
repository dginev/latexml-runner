# latexml-runner
Rust runner for high-performance conversions with latexmls.

[![Build Status](https://github.com/dginev/latexml-runner/workflows/CI/badge.svg)](https://github.com/dginev/latexml-runner/actions?query=workflow%3ACI) [![License](http://img.shields.io/badge/license-MIT-blue.svg)](https://raw.githubusercontent.com/dginev/latexml-runner/master/LICENSE)
[![crates.io](https://img.shields.io/crates/v/latexml-runner.svg)](https://crates.io/crates/latexml-runner)

This executable is useful over the native Perl executables of latexml if:
 - You have a large number of tasks with a shared set of TeX/LaTeX packages and preambles.
 - The tasks are small enough that latexml's overhead is a performance bottleneck (e.g. single formulas or abstracts/paragraphs).
 - Your system installation allows for a custom perl plugin
 - concretely [latexmls v1.5.0](https://github.com/dginev/LaTeXML-Plugin-latexmls), a socket server for LaTeXML.

If your conversion task requires a distributed setup and/or is unable to preload most dependencies, this crate won't offer you much of a speedup. For such cases, consider [LaTeXML::Plugin::Cortex](https://github.com/dginev/LaTeXML-Plugin-cortex) as one alternative.

### Demo

You can try it out by using the publick docker image and avoid any installation headaches.

In the following example, we use an invocation useful for converting math in sites such as
StackExchange and Wikipedia:

```bash
$ for i in {1..200}; do echo "\sqrt{x}+\frac{1}{2}=0" >> formula_latex.txt; done
 
$ time docker run --cpus="2.0" --memory="8g" \
-v "$(pwd)":/workdir -w /workdir \
latexml/latexml-runner:latest \
-i formula_latex.txt -o formula_xml.csv -l formula_status.log \
--preload=LaTeX.pool --preload=bm.sty --preload=texvc.sty \
--preload="literal:\\let\\theequation\\relax" \
--whatsin=math --whatsout=math --pmml --cmml --mathtex --format=html5 \
--nodefaultresources --timeout=30 
```

Should complete in e.g. 9.5 seconds on a `Intel(R) Xeon(R) Gold 6148 CPU @ 2.40GHz`. 

Importantly, the `formula_status.log` file should contain two hundred zeros, one on each line, to signal that the conversions are robustly finishing error-free. In other words, that the harness and `latexmls` are communicating correctly.