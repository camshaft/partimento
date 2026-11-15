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
registry = "registry/identifiers.toml"

[[system]]
name = "core"
first = 1
last = 99

[contracts]
name = "contracts"
first = 100
last = 199

[foundation]
name = "foundation"
first = 900
last = 999
```

- `prefix` starts each requirement identifier, `<prefix>-<rfc>-<n>`, such as `EX-0001-001`:
  the RFC's number in four digits and the requirement's number in three digits or more.
- `levels` lists the key words a statement uses, such as `MUST`, `SHOULD` and `MAY`.
- `implementation_terms` lists the words that name how a system is built. The check warns about
  a statement that uses one of them, since a requirement states what a system does and names
  such a part by its category.
- `[paths]` sets the directory of the RFC sources, of the rendered sections and of the
  requirement files, and may set `registry`, the path of the registry of identifier kinds, a file
  outside those three directories. A set whose `[paths]` leaves `registry` out doesn't hold a
  registry.
- Each `[[system]]`, `[contracts]` and `[foundation]` sets a group of RFCs: its name and the
  range of its RFC numbers, `first` to `last`. `[contracts]` holds the RFCs of the contracts
  between the systems, and `[foundation]` the RFCs every system builds on. The check reads the
  groups to find a requirement that reaches across the boundary between two systems, and to
  check each RFC's area.

### The RFCs

`index.toml`, in the RFCs' directory, lists every RFC of the set by its number, title and area:

```toml
[[rfc]]
number = 1
title = "Greetings"
area = "core"
```

An RFC's area names the group whose range holds its number, so RFC 1 above is in the `core`
system. An RFC is written where the set holds its directory, and planned where the index lists
it and the set doesn't hold a directory of its number. The RFC's directory decides its state, so
a row that still stores `state` gets a warning.

Each written RFC has a directory `<nnnn>-<slug>` that holds its `rfc.toml`:

```toml
number = 1
title = "Greetings"
area = "core"
abstract = "How the program greets a person when it starts and when it stops."
retired = []
```

`rfc.toml` and the index agree on the RFC's title and area. `retired` lists the identifiers the
RFC no longer holds, which no requirement uses again. A contract RFC, one whose number is in the
`[contracts]` range, also holds `contract_version`, a whole number from 1 that its writers raise
with each change of the contract; an RFC of another group doesn't hold it.

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
binds_to = "system"
statement = "The program MUST print \"hello\" when it starts."
rationale = "The greeting is how the person sees that the program started."

[[requirement]]
id = "EX-0001-002"
level = "MUST"
polarity = "forbidden"
binds_to = "system"
statement = "The program MUST NOT print the greeting to the error stream."
rationale = "A person who reads the error stream reads only the errors."
```

A section needs `anchor`, `title`, `problem` and `motivation`, and may hold `prose`. The anchor
holds ASCII letters, digits, `-` and `_`, and the title is one line.

A section that specifies a kind of self-describing identifier names it in a `[[kind]]` table
beside its requirements:

```toml
[[kind]]
name = "a greeting"
formats = [1, 2]
```

A kind table holds `name` and `formats`, and no other field. `name` is a text that no other kind
table of the set holds, the name the RFC uses for the kind of record, and `formats` is a list of
one or more whole numbers of at least 1, none of them twice: the number of each form of the
kind's identifier that the RFC specifies.

A requirement needs `id`, `level`, `polarity`, `binds_to`, `statement` and `rationale`.
`polarity` is `required` or `forbidden`, and the statement's key words match the level and the
polarity: `MUST` for a required `MUST`, `MUST NOT` for a forbidden one. `binds_to` is `system`,
for a requirement on what a system does, or `process`, for one on how its writers work, and
`rationale` says why the requirement holds, in text that isn't empty. A requirement may also
hold:

- `label`;
- `relations`, each a table with a `kind` and a `target` requirement, where the kind is
  `depends_on`, `implies`, `refines`, `conflicts_with` or `supersedes`;
- `compose`, one of `atomic`, `all_of`, `any_of`, `exactly_one` and `unless`, with its `parts`;
- `conditions` and `logic`;
- `params`, each a table that holds the parameter's `name`, its `type`, its `unit`, and either
  its `bound` (`min`, `max` or `exact`) with its `value`, or its `source`.

A parameter's `type` is `count`, `duration`, `size`, `rate` or `fraction`, and its `unit` is `s`
for a duration, `B` for a size and `per_s` for a rate; a count and a fraction don't hold a unit.
The value of a count or a size is a whole number, and the value of a fraction is from 0 to 1. A
parameter without a value names where its value comes from: `source = "deployment"`, set where
the system runs, or `source = "contract"`, read from a parameter of the same name in a contract
RFC. A parameter with a source doesn't hold a `bound`: the bounds that other parameters of the same
name state hold on its value. The check that a name has one type and one unit reads it, and the
comparisons of minimum, maximum and exact bounds leave it out.

The last of two or more levels, such as `MAY`, is permissive: a requirement at that level allows
something, so it adds neither a formula nor a condition to the formula set that the consistency
checks read, from itself, its composition, its logic, its relations or its conditions. Its fields
are still checked, and the other checks of the requirements still read it: its `depends_on`,
`implies` and `refines` relations count in the chains that lead a required requirement to a
forbidden one, its `implies` and `refines` relations count in cycles, and its parameters' bounds
count with the others'.

### The registry

The registry lists each kind of self-describing identifier that an RFC of the set specifies, one
`[[entry]]` for each format of the kind:

```toml
[[entry]]
code = "g"
identifies = "a greeting"
format = 1
body_length = { min = 8, max = 8 }
defined_by = "EX-0001-001"
```

An entry holds `code`, `identifies`, `format`, `body_length` and `defined_by`, and no other field.
`code` is a text that no other entry holds, and `identifies` a text that states the kind of record
the code identifies; no two entries hold the same `identifies` and `format`. `format` is a whole
number of at least 1, the number of the identifier's form that the code names. `body_length` holds
`min` and `max` alone, the fewest and the most characters of a body, each a whole number, `min` at
least 1 and not greater than `max`. `defined_by` is the identifier of a requirement of a written
RFC that no RFC retires. A registry that doesn't hold an entry yet is a file of comment lines
alone.

The registry holds an entry of each format of each kind table, whose `identifies` is the table's
`name` and whose `format` is that format, and each entry matches a kind table: its `identifies` is
the `name` of a kind table, its `format` is one of that table's `formats`, and its `defined_by`
names a requirement of the section file that holds that table. A set whose spec.toml doesn't set
`paths.registry`, or whose registry file is missing, doesn't hold an entry, so it fails at each
format of each kind table it holds.

A change of the set adds entries, and doesn't remove one or change its `code`, `identifies`,
`format` or `body_length`: `partimento diff` reports each entry of the base that the head
removes or changes, with the rule `registry-kept`, and exits with 1. Where the head's registry
isn't UTF-8 or doesn't parse, it reports the findings of that read instead, and exits with 1.

## partimento check

```text
partimento check [--root <dir>] [--format text|json] [--final] [--cache <dir>]
                 [--formulas <file>]
```

`check` reads the tree at `--root`, the current directory where it is left out, and prints
each finding on a line of its own:

```text
<severity>[<rule>] <file>:<line>: <requirement>: <message>
```

The severity is `error` or `warning`, and a finding that isn't about one requirement leaves out
the requirement's part. The check runs:

- the format rules: the form, uniqueness and retirement of each identifier, the key words of
  each statement, the fields of each section and requirement and their values, each parameter's
  type, unit, value and source, the form of each section's anchor and title, each RFC's area and
  `contract_version`, the agreement of `rfc.toml` and the index, and the implementation terms;
- the fields of each kind table and their values, with the rule `kind`;
- the registry's rules: that the file is there where spec.toml sets `paths.registry`, the fields
  of each entry and their values, that each entry matches a kind table, and that the registry
  holds an entry of each format of each kind table;
- the checks of the requirements: that their formulas hold together, which a SAT solver decides,
  with a smallest set of requirements that can't hold together where they don't; that each
  condition can hold while the rest of the set does; each relation's target; the boundary
  between systems; cycles of `implies` and `refines`; a required requirement whose relations
  lead to a forbidden one; and the bounds of each parameter;
- that each generated file matches what `partimento render` writes from the sources, where the
  format rules pass.

It exits with 0 where no finding is an error, 1 where one is, and 2 where the arguments are
wrong or a file can't be read or written.

- `--final` makes a relation whose target no requirement holds yet an error, where it is a
  warning otherwise: give it once every RFC the set plans is written.
- `--format json` prints the findings as a JSON array, each with its `severity`, `rule`,
  `file`, `line`, `requirement` (`null` where it isn't about one) and `message`.
- `--cache <dir>` keeps what the check builds from each section file in `<dir>`, a file for
  each key, and reads it back in later checks. A check returns the same findings with the cache
  and without it.
- `--formulas <file>` writes the formula set that the consistency checks read to `<file>` as
  JSON before it prints the findings: an object with the arrays `atoms`, `formulas` and
  `conditions`. Each atom holds its `id`, `rfc`, `section`, `level` and `polarity`, `null` where
  its requirement doesn't hold the field. Each formula holds its `formula`, an object whose one
  key is its kind, `atom`, `not`, `and`, `or`, `implies` or `iff`; its `origin`, the
  `requirement`, `file` and `line` it comes from; and its `relation`, the `kind` and `target` of
  the relation it states, or `null`. Each condition holds its `atom` and its `origin`.

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
at `<head>`, and each contract RFC whose `contract_version` differs between them, as
`version <rfc> <base version> <head version>`, and exits with 0. Where either tree breaks a
format rule, it prints that tree's failing findings instead and exits with 1. It exits with 2
where the arguments are wrong or a file can't be read.

## The library

```toml
[dependencies]
partimento = { version = "0.2", default-features = false, features = ["batsat"] }
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
