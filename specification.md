# Inne Language Specification

Like Python, Inne allows top-level code.

The syntax is roughly based on Rust's. Statements are newline-delimited as in Go. By convention, variables and functions are written in `snake_case`, and user-defined types in `PascalCase`.

Example:

```
var counter = 0

func greet(name: string) {
    // Increment the global counter.
    counter += 1
    
    print("Hello, {name}!")
}

greet("User")

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

These are essentially Python format strings (f-strings), but without the leading `f`

Example: `Hello, {name}!`

Braces can be escaped using `\{` to prevent formatting.

Example: `"This is a brace: \{"`

### Characters

UTF-32 characters. The following escape codes are supported: `\" \' \\ \n \r \t \{ \}`

Example: `'d'`

Example: `'\n'`

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

Structs may have generics.

Example:
```
struct Node<T> {
    value: T,
    left: Option<Node<T>>,
    right: Option<Node<T>>
}
```

Structs are instantiated using `Name:{ ... }` syntax.

Example:
```
Name:{
    name0: value0,
    name1: value1,
}
```

Generics are automatically inferred from field values.

### Enums

Enums have variants, which may contain a payload.

Example:
```
enum Name {
    Unit,
    WithPayload(int)
}
```

Enums are instantiated using `Name:Variant` or `Name:Variant(payload)` syntax.

Example: `Name:Unit`

Example: `Name:WithPayload(5 + 6)`

Enums may have generics.

When instantiating, generics are inferred when possible. Unused generics have the special compiler-internal type `?`, which can be converted to any other type.

Example:
```
enum Result<O, E> {
    Ok(O),
    Err(E),
}

Result:Ok(int)      // has type Result<int, ?>
Result:Err("Error") // has type Result<?, string>
```

## Functions

Functions may not be defined inside other functions.

Global variables are not accessible from inside functions.

Functions are not first-class, so they cannot be passed to other functions.

The syntax follows Rust's, but uses the `func` keyword instead of `fn`.

Explicit returns (with the `return` keyword) are mandatory.

A `return` without value is allowed in the global scope.

Functions without a return type specified implicitly return `Nothing`.

Functions may be annotated with generic types. A generic used in the return type must also be used in at least one parameter's type.

Example:
```
func greet(name: string) {
    print("Hello, {name}!")
}
```

Example:
```
func square(number: int) -> int {
    return number * number
}
```

Example:
```
func create_list<T>(item: T) -> list<T> {
    return [item]
}
```

### Built-in Functions

#### Print

`print(...)` takes any number of arguments of any (mix of) types and writes them to stdout them with a space in between and followed by a newline.

#### Panic

`panic(message)` exits the program with exit code 1 and writes the provided message to stderr.

## Operators

Boolean operators: `and`, `or`, `not`

Arithmetic operators: `+`, `-`, `*`, `/`

## Variables

A variable definition is written as `var name = value`.

Tuples, lists, and maps can be destructured as in Python.

## Assignments

Variables and indexed lists/maps can be used as assignment target.

The target's type must be the same as the value's type. There is no type coersion.

Example: `variable = value` or `list[index] = value` or `map[key] = value`

All binary operators, except `and`, `or`, and `not`, can be used in an assignment.

Example: `number += value` or `string += value` or `number /= value`

## Control-flow

Braces are mandatory for each kind of control-flow statement, unlike in C.

Conditions are always boolean expressions.

### If-statements

`if condition { ... }` as in Rust.

`if condition { ... } else { ... }` and `if condition0 { ... } else if condition1 { ... }` are also allowed.

### Match-statements

Enum values, integers, booleans, floats, and strings can be matched in a `match` statement.

`_` is used to indicate "any other value", comparably to an if-statement `else`.

Example:
```
var none = Option:None
match none {
    Some(value) { print(value) }
    None { print("None") }
}
```

Example:
```
match "yes" {
    "yes" { print("YES!") }
    "no" { print("NO!!!") }
    _ { print("unknown") }
}
```

Example:
```
match n {
    5 { print("five") }
    3 { print("three") }
    _ {}
}
```

### For-loops

`for variable in iterator { ... }` as in Rust.

Only ranges (`start to end`), lists, and maps can be used as iterators.

Using a range as iterator yields integers, using a list yields its elements, and using a map yields its keys.

Ranges are a special construct that can only be used in a for-loop. Their `end` is exclusive.

Example:
```
for i in 0 to 10 {
    print(i)
}
```

### While-loops

`while condition { ... }` as in Rust.

### Infinite loops

`loop { ... }` as in Rust.

### Loop Control-Flow

Loops can be exited with `break` and the rest of an iteration can be skipped with `continue`.

## Imports

Other files can be imported using the `import` keyword.

The compiler first searches for dependencies in the project specification with the given name, including the standard library and the project itself (to access the project root).  
Failing to find a dependency, it looks for a file in the current directory.  
Following this, it looks for a file in a folder with the same name as the current file (minus the file extension). This is called a submodule.

Example: `import file`

Submodules can be accessed with `/`

Example: `import file/submodule/subsubmodule`

## Projects

An Inne project consists of all `.inne` files in the same folder as a `main.inne` file (the project root), as well as in subfolders.

Except in `main.inne`, variables and expressions in the global scope (i.e. outside of functions) may not depend on code from other files in the same project.
