# partimento

partimento checks and renders specifications written as structured RFCs. Each requirement of
an RFC is a TOML record with an identifier, a key word and a statement. `partimento check`
reports where the set breaks a format rule or where its requirements can't hold together, and
`partimento render` writes each section as Markdown, beside the requirement files that a
requirements-tracing tool such as [duvet](https://github.com/awslabs/duvet) reads.

## Install

From crates.io:

```sh
cargo install partimento
```

From this repository:

```sh
cargo install --git https://github.com/camshaft/partimento
```

partimento builds with Rust 1.88 or later. Each GitHub release also carries the `partimento`
program built for Linux on x86_64 and aarch64, as `partimento-<target>.tar.gz` with its SHA-256
sum in `partimento-<target>.tar.gz.sha256`.

## A specification tree

A tree holds `spec.toml` at its root, the RFC sources, and the files that `partimento render`
writes from them:

```text
spec.toml
rfcs/
  index.toml
  0001-greetings/
    rfc.toml
    01-greeting.toml
    02-reply.toml
rendered/                   written by `partimento render`
  0001-greetings/
    01-greeting.md
    02-reply.md
.duvet/requirements/        written by `partimento render`
  0001-greetings/
    01-greeting/
      EX-0001-001.toml
      EX-0001-002.toml
    02-reply/
      EX-0001-003.toml
```

### spec.toml

`spec.toml` sets the identifier prefix, the key words and the paths. partimento reads every one
of these values from the file:

```toml
prefix = "EX"
levels = ["MUST", "SHOULD", "MAY"]
implementation_terms = ["sqlite", "message queue"]

[paths]
rfcs = "rfcs"
rendered = "rendered"
requirements = ".duvet/requirements"

[[system]]
name = "core"
first = 1
last = 99

[contracts]
first = 100
last = 199
```

- `prefix` starts each requirement identifier, `<prefix>-<rfc>-<n>`, such as `EX-0001-001`:
  the RFC's number in four digits and the requirement's number in three digits or more.
- `levels` lists the key words a statement uses, such as `MUST`, `SHOULD` and `MAY`.
- `implementation_terms` lists the words that name how a system is built. The check warns about
  a statement that uses one of them, since a requirement states what a system does and names
  such a part by its category.
- `[paths]` sets the directory of the RFC sources, of the rendered sections and of the
  requirement files.
- Each `[[system]]` sets a system's name and the range of its RFC numbers, and `[contracts]`
  the range of the RFCs that hold the contracts between the systems. The check reads them to
  find a requirement that reaches across the boundary between two systems.

### The RFCs

`index.toml`, in the RFCs' directory, lists every RFC of the set, with `state` `written` or
`planned`:

```toml
[[rfc]]
number = 1
title = "Greetings"
area = "core"
state = "written"
```

Each written RFC has a directory `<nnnn>-<slug>` that holds its `rfc.toml`:

```toml
number = 1
title = "Greetings"
area = "core"
abstract = "How the program greets a person when it starts and when it stops."
retired = []
```

`retired` lists the identifiers the RFC no longer holds, which no requirement uses again.

Beside `rfc.toml`, the directory holds one file for each section, `<nn>-<slug>.toml`:

```toml
[section]
anchor = "greeting"
title = "Greeting"
problem = "A person who starts the program doesn't know that it runs."
motivation = "A greeting tells the person that the program runs."
prose = "The program greets the person who starts it."

[[requirement]]
id = "EX-0001-001"
level = "MUST"
polarity = "required"
statement = "The program MUST print \"hello\" when it starts."

[[requirement]]
id = "EX-0001-002"
level = "MUST"
polarity = "forbidden"
statement = "The program MUST NOT print the greeting to the error stream."
```

A section needs `anchor`, `title`, `problem` and `motivation`, and may hold `prose`. The anchor
holds ASCII letters, digits, `-` and `_`, and the title is one line.

A requirement needs `id`, `level`, `polarity` and `statement`. `polarity` is `required` or
`forbidden`, and the statement's key words match the level and the polarity: `MUST` for a
required `MUST`, `MUST NOT` for a forbidden one. A requirement may also hold:

- `label` and `rationale`;
- `relations`, each a table with a `kind` and a `target` requirement, where the kind is
  `depends_on`, `implies`, `refines`, `conflicts_with` or `supersedes`;
- `compose`, one of `atomic`, `all_of`, `any_of`, `exactly_one` and `unless`, with its `parts`;
- `conditions`, `logic` and `params`;
- `binds_to`, `system` or `process`.

## partimento check

```text
partimento check [--root <dir>] [--format text|json] [--final] [--cache <dir>]
```

`check` reads the tree at `--root`, the current directory where it is left out, and prints
each finding on a line of its own:

```text
<severity>[<rule>] <file>:<line>: <requirement>: <message>
```

The severity is `error` or `warning`, and a finding that isn't about one requirement leaves out
the requirement's part. The check runs:

- the format rules: the form, uniqueness and retirement of each identifier, the key words of
  each statement, the fields of each section and requirement and their values, the form of each
  section's anchor and title, and the implementation terms;
- the checks of the requirements: that their formulas hold together, which a SAT solver decides,
  with a smallest set of requirements that can't hold together where they don't; that each
  condition can hold while the rest of the set does; each relation's target; the boundary
  between systems; cycles of `implies` and `refines`; a required requirement whose relations
  lead to a forbidden one; and the bounds of each parameter;
- that each generated file matches what `partimento render` writes from the sources, where the
  format rules pass.

It exits with 0 where no finding is an error, 1 where one is, and 2 where the arguments are
wrong or a file can't be read.

- `--final` makes a relation whose target no requirement holds yet an error, where it is a
  warning otherwise: give it once every RFC the set plans is written.
- `--format json` prints the findings as a JSON array, each with its `severity`, `rule`,
  `file`, `line`, `requirement` (`null` where it isn't about one) and `message`.
- `--cache <dir>` keeps what the check builds from each section file in `<dir>`, a file for
  each key, and reads it back in later checks. A check returns the same findings with the cache
  and without it.

## partimento render

```text
partimento render [--root <dir>] [--format text|json]
```

`render` writes the generated files of the tree at `--root`: a Markdown file for each section,
under `paths.rendered`, and a requirement file for each requirement, under
`paths.requirements`. It writes each generated file that differs from the tree, removes each
file under those two directories that the render doesn't write, prints nothing and exits with 0.
Where the set breaks a format rule, it prints the findings of the format rules, writes nothing
and exits with 1. It exits with 2 where the arguments are wrong or a file can't be read or
written.

The section above renders as `rendered/0001-greetings/01-greeting.md`:

```markdown
## Greeting {#greeting}

The program greets the person who starts it.

### EX-0001-001 {#EX-0001-001}

The program MUST print "hello" when it starts.

### EX-0001-002 {#EX-0001-002}

The program MUST NOT print the greeting to the error stream.
```

and its first requirement as `.duvet/requirements/0001-greetings/01-greeting/EX-0001-001.toml`:

```toml
target = "rendered/0001-greetings/01-greeting.md#EX-0001-001"

[[spec]]
level = "MUST"
quote = "The program MUST print \"hello\" when it starts."
```

Run `partimento render` after each change of the sources, and commit the generated files with
them: `partimento check` reports each generated file that differs from what the render writes.

## partimento diff

```text
partimento diff <base> <head> [--format text|json]
```

`diff` prints each requirement added, changed or retired from the tree at `<base>` to the tree
at `<head>`, and exits with 0. Where either tree breaks a format rule, it prints that tree's
failing findings instead and exits with 1. It exits with 2 where the arguments are wrong or a
file can't be read.

## The library

```toml
[dependencies]
partimento = { version = "0.1", default-features = false, features = ["batsat"] }
```

The library reads only the `Tree` it is given: the tree's files, by their paths relative to its
root, with `/` between the parts of a path. It doesn't read files, start processes, read the
environment or the clock, or print anything, and two checks of the same tree return the same
report.

```rust
use partimento::{check_default, Options, Tree};

fn passes(tree: &Tree) -> bool {
    let report = check_default(tree, &Options::default());
    for finding in &report.findings {
        println!("{finding}");
    }
    !report.failed()
}
```

`render` returns a tree of the generated files, `diff` the changes between two trees, and
`check_cached` runs a check with a `Cache` of its own.

## Features

- `cli`, on by default: the `partimento` program.
- `batsat`, on by default: the [batsat](https://crates.io/crates/batsat) SAT solver.
- `varisat`: the [varisat](https://crates.io/crates/varisat) SAT solver.

The library needs `batsat` or `varisat`. `check_default` uses batsat where its feature is on,
and varisat otherwise; `check` takes the backend as a type parameter.

## License

MIT, as [LICENSE](LICENSE) says.
