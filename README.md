# Office Claude

Written by Claude allow our customer support the ability to ask claude questions about our codebases with out having to setup their own machines with claude and the code.  Provides a web interface for claude coding.

## Description

Setup the config file with the location of your codebases.  Then run the `cargo run` or build it and set it up to run as a service.  If you have claude code have claude setup this system for you.

## How it works under the hood

When a user logs in, they can click on which repository they want to work with.  The server then copies that code directory into a temporary folder and opens a new branch in that folder. Then an in browser terminal is opened with claude running.  The user can ask claude questions about the code base.  They can even request edits and ask claude to make a pull request with their changes, (this requires having git and gh setup on the machine running the server).
