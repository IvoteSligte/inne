# Inne Language Specification

Like Python, Inne allows top-level code. Global variables are accessible from inside functions.

The syntax is roughly based on Rust's. Semicolons are optional, but recommended. By convention, variables and functions are written in `snake_case`, and user-defined types in `PascalCase`.

Example:

```
var counter = 0;

func greet(name: string) {
    // Increment the global counter.
    counter += 1;
    
    print("Hello, {name}!");
}

greet("User");

```

## Types

Types may not be defined inside functions.

### Integers

64-bit signed integers.

`0` `1000` `9223372036854775807i64`

Integer arithmetic is checked. The program crashes with a suitable error message on undefined behavior.

### Strings

UTF-8 strings.

Example: `"abcd\n\t\"Hello, World!""`

These are essentially Python format strings (f-strings), but without the leading `f`.

Example: `Hello, {name}!`

Braces can be escaped through duplication to prevent formatting.

Example: `"This is a brace: {{"`

### Characters

UTF-32 characters.

`'d'` `'\n'`

### Ranges

A range has the syntax `start..end`. `start` and `end` are integers. The end is exclusive.

### Lists

Empty list: `[<type>]`

Non-empty list: `[0, 1, 2]`

This means that all list values have known types and do not require type inference.

Lists can be indexed with integers (e.g. `list[index]`) and sliced with ranges (e.g. `list[start..end]`).

### Maps

Empty map: `{<key_type, value_type>}`

Non-empty map: `{key0: value0, key1: value1}`

This means that all map values have known types and do not require type inference.

Any type can be used as a key or value (e.g. `map[key]`).

### Tuples

Rust tuples.

### Structs

Rust C-style structs.

Example:
```
struct Name {
    name0: type0,
    name1: type1,
}
```

The trailing comma is optional for the last field.

Structs are instantiated using `Name { ... }` syntax.

Example:
```
Name:{
    name0: value0,
    name1: value1,
}
```

### Enums

Rust C-style enums (i.e. tagged unions).

Example:
```
enum Name {
    Unit,
    Struct {
        name0: type0,
        name1: type1,
    }
}
```

Enums are instantiated using `Name::Variant` syntax.

Example: `Name::Unit`

Example: 
```
Name::Struct {
    
}
```

## Functions

Functions may not be defined inside other functions.

Functions are not first-class, so they cannot be passed to other functions.

The syntax follows Rust's, but uses the `func` keyword instead of `fn`.

Explicit returns (with the `return` keyword) are mandatory.

### Built-in Functions

#### Print

`print(...)` takes any number of arguments of any (mix of) types and prints them with a space in between and followed by a newline.

## Operators

Boolean operators: `and`, `or`, `not`

Arithmetic operators: `+`, `-`, `*`, `/`

## Variables

A variable definition is written as `var name = value;`.

Tuples, lists, and maps can be destructured as in Python.

In control-flow statements, `var Enum::Variant(name) = value` can be used as condition as in Rust.

## Assignments

Variables and indexed lists/maps can be used as assignment target.

The target's type must be the same as the value's type. There is no type coersion.

Example: `variable = value;` or `list[index] = value;` or `map[key] = value;`

All binary operators, except `and`, `or`, and `not`, can be used in an assignment.

Example: `number += value;` or `string += value;` or `number /= value;`

## Control-flow

Braces are mandatory for each kind of control-flow statement, unlike in C.

### If-statements

`if condition { ... }` as in Rust.

`if condition { ... } else { ... }` and `if condition0 { ... } else if condition1 { ... }` are also allowed.

### For-loops

`for value in iterator { ... }` as in Rust.

Only ranges, lists, and maps can be used as iterators.

### While-loops

`while condition { ... }` as in Rust.

### Infinite loops

`loop { ... }` as in Rust.

## Imports

Other files can be imported using the `import` keyword.

The compiler first searches for dependencies in the project specification with the given name, including the standard library and the project itself (to access the project root).  
Failing to find a dependency, it looks for a file in the current directory.  
Following this, it looks for a file in a folder with the same name as the current file (minus the file extension). This is called a submodule.

Example: `import file;`

Submodules can be accessed with `::`

Example: `import file::submodule::subsubmodule;`
