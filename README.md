# Office Claude

Written by Claude to allow our customer support the ability to ask claude questions about our codebases without setting up their own machines with everything necessary to view the code and run claude.  Essentially this provides a web interface for claude coding.

## Setup

Setup the config file with the location of your codebases.  Then run the `cargo run` or build it and set it up to run as a service.  If you have claude code have claude setup this system for you.  I recommend running it on a small pc on the local network at your office, you could even setup the pc with tailscale for remote access.

## How it works under the hood

When a user logs in, they can click on which repository they want to work with.  The server then copies that code directory into a temporary folder and opens a new branch in that folder. Then an in-browser terminal is opened with claude running.  The user can ask claude questions about the code base.  They can even request edits and ask claude to make a pull request with their changes, (this requires having git and gh setup on the machine running the server).
