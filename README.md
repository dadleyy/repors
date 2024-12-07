# `repors`: rust `repo`

This repository contains a pure rust implemention of a tool that is meant to _partially_
implement the functionality provided by the [`repo`][repo-link] provided by google. 

The primary motivation for this implementation was the need for a much simpler tool that
only concerns itself with the downloading/cloning of repositories listed in _some manifest
file_. `repors` is _not_ meant to be a 100% complete alternative; whatever monorepo-like
management functionality is implemented by the `repo` command line tool, `repors` is not
designed for.

Ultimately, at the end of the day, this tool was made with the hope that it might help folks
working with the [openembedded] tools + [yocto]. Using a `<manifest>.xml` file to list the
layer dependencies in some top level project seems relatively common (see: 
[stm's oe-manifest][stm] repo).

---

No fancy build steps, just `cargo build`:

```
$ cargo build
$ ./target/debug/repors --help
Usage: repors <COMMAND>

Commands:
  execute  This command will actually perform the git cloning of all the repositories listed in a manifest xml file
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```


---

The majority of this work is combining the functionality provided by the [clap], [git2], and [quick-xml]
crates. Big thanks to those maintainers.

[repo-link]: https://gerrit.googlesource.com/git-repo
[openembedded]: https://www.openembedded.org/wiki/Main_Page
[yocto]: https://www.yoctoproject.org/
[stm]: https://github.com/STMicroelectronics/oe-manifest/blob/791a7199cd9469ebab1a867990efbe75bda95bf8/default.xml
[clap]: https://github.com/clap-rs/clap
[git2]: https://github.com/rust-lang/git2-rs
[quick-xml]: https://github.com/tafia/quick-xml
