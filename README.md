[![Run on Repl.it](https://repl.it/badge/github/openflowlabs/libips)](https://repl.it/github/openflowlabs/libips)

# Library used by the Image Packaging System

This repository contains all modules and functions one needs to implement an Image Packaging System based utility.

Be that a server, client or other utilities. 

Includes Python bindings with PyO3.

This project is intended to gradually replace the 
[current python based implementation of IPS](https://github.com/openindiana/pkg5). 
Most things are documented in the [docs](https://github.com/OpenIndiana/pkg5/tree/oi/doc) directory 
but some things have been added over the years which has not been properly documented. Help is welcome 
but be advised, this is mainly intended for use within the illumos community and it's distributions.
Big changes which are not in the current IPS will need to be carefully coordinated to not break the current
IPS.