# Rust implementation of Cowsay

### Building from Source

`cargo build`

This creates `cowsay` and `cowthink` binaries. The test to say can be passed in as a command line parameter, or can be piped in from another process.

### Running

The defaults are the same as for classic cowsay, all the same command line parameters are supported
(run `./cowsay -h` for a full list of command line parameters)

`./cowsay "This is a test run"` or `echo "This is a test run" | ./cowsay`
```
 ___________________
< This is a test run >
 -------------------
        \   ^__^
         \  (oo)\_______
            (__)\       )\/\
                ||----w |
                ||     ||
```

(Included default Cowfile)

### Custom cows

Both binaries include a list of custom cowfiles with alternative characters (display full list with `./cowsay -l`)

`./cowsay -f tux "This is a test run"`
```
___________________
< This is a test run >
 -------------------
   \
    \
        .--.
       |o_o |
       |:_/ |
      //   \ \
     (|     | )
    /'\_   _/`\
    \___)=(___/
```
(Custom included Cowfile)


### Modern cowsay (kitty only)

I have included a spiced-up version of cowsay using unicode box characters and png images (currently only supporting kitty graphics protocol).
While any png can be passed in with the `-f` parameter, by default a cat and Pipboy are provided.
(Pipboy is a work in progress that will feature animations at some point)

`./cowsay -m "This is a test run"` (or `./cowsay -c "This is a test run"` for Pipboy)

This can also be combined with the `-f` parameter for custom PNGs:

`./cowsay -m -f random.png "This is a test run"`

### Credits

This is a fork of the rust cowsay repo by Matt Smith, all credits to Matt. I am a rust amateur and worked on this as a trial run to see if rust suits me
(that also means the code is not as good / refined as it could be... feel free to make suggestions on improvements).
