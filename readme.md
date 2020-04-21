# run 

> Minimal task runner.

run is an _extremely_ minimal task runner that supports a shell-lite syntax.

An example run file:

```
// Build binary paramaterized by some C library.
$cc -o tmp.exe foo.c $lib 
// Run it with some arguments in a shell pipeline.
tmp.exe --flag-one $flag-one | tail $1
// Cleanup afterward.
rm *.exe 
```

Run it like this:

`run build.run -cc gcc -lib foobar.h -flag-one foo 10`

### Syntax

Very simple. '$' denotes a variable. Words declare __named__ variables, numbers 
declare __positional__ variables. 
Comments are C-like, denoted by '//'.

For example: 

```
// Positional variables.
echo $1 $2 $3
// Named variables.
echo $foo $bar $baz
```

Remarks  
- For personal use (experimental, use at your own risk).    
- Highly portable because it's minimal.   
- Sick of looking at powershell, bash, make, and co.  
- Does the 80% solution of running paramatarised commands in sequence.   
- Features added via personal workflow.   
- Not trying to support the universe.
- No turing complete language included.   