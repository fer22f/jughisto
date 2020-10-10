Jughisto
===

Work in progress competitive programming judge that is compatible with Polygon packages.

## How to Run

* Make sure the `isolate/` submodule has been fetched
* Go to `isolate/` folder and `make isolate`, then `make install`
* Run the app with `sudo -E PATH=$PATH cargo run` (so it copies over the environment variables from your user)

## Features

* Polygon contest package support
* Web backend made in Rust with Rocket+Diesel
* Isolation made using [isolate](https://github.com/ioi/isolate)
* Lightweight server-side rendered frontend with SSE updates
* Docker support

## TODO

* Internationalization
* Parallelize judging process

## Not Planned

* Support for multiple file programs

## Planned

* Problem creation platform
