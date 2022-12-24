# ron: donno in Rust

Install: `cargo install --path .`

Usage: `ron -h`

## Development

Here the key compoments of ron, and the positions where it's discussed in
clr2022 are listed:

* Writing and running integration tests: p6
* Writing and running unit tests: chapter 4
    + Create a module for tests: chapter 5
    + Fake a filehandle for testing: chapter 5
* Define data structure: chapter 3
* CLI building and parsing: chapter 2
    + Get value of the argument
        - Parse a string into a number: chapter 4
    + Validate data type of the argument
    + Constrain possible values for command-line arguments: chapter 7
    + Produce highlighted text in the terminal: chapter 13
    + Use text tables to create aligned columns of output: chapter 14
* File path management:
    + Use the Path and PathBuf structs to represent system paths: chapter 12
* Write text files
    + Write to a file or STDOUT: chapter 6
    + Find today’s date and do basic date manipulations: chapter 13
* Check the existence of a file: chapter 3
* Parse text files
    + Use std::cmp::Ordering when comparing strings: chapter 10
    + Parse records of text spanning multiple lines from a file: chapter 12
* Run external system command, e.g.: `vim`, `pandoc`, etc. in function: p8
* Search texts in files:
    + Simple search in multiple text files:
        - Use a regular expression to find a pattern of text: chapter 7
        - Using a case-sensitive regular expression: chapter 9
        - Seek to a line or byte position in a filehandle: chapter 11
    + Complex search in multiple text files
        - Chain multiple filter, map, and filter_map operations: chapter 7
    + Incremental search in current searching result
* Configuration management:
    + JSON file management:
* Temp file management:
    + Use temporary files: chapter 6
* Compile source codes on Linux and Windows:
    + Compile code conditionally when on Windows or not: chapter 7
* Release binary executable:
    + Build a release binary with Cargo: chapter 11
* Publish package to public registry:

