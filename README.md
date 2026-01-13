Description
===========

"Mouseless navigation". Keyboard navigation and window tiling for Hyprland. 

`Hlwinter` installs two programs, `wint` and `winj`.

The first program, `winj`, allows to use keyboard to switch between windows and workspaces. 
It shows a menu with the colored list of windows, marked by letters a-z. 
Colors are configurable. Pressing the key a-z "teleports" to that window. 
Pressing the space bar brings up the previous window. Pressing 1-9 "teleports" to the corresponding workspace. 

The second program, `wint`, allows to tile windows using predefined tiling schemes. 

I use patterns from [colourlovers.com](http://www.colourlovers.com/lover/albenaj) for the window background:

![Screenshot](screenshot.png "Screenshot")


Installation
============

With Nix package manager
------------------------

    nix profile install github:amkhlv/hlwinter


This installs two programs: `winj` and `wint`.

On Debian
---------

Need to install [Cargo](https://www.rust-lang.org/tools/install), 
and some system libraries:

    sudo apt-get install build-essential  libgtk-3-dev libgdk-pixbuf-2.0-dev libatk1.0-dev libpango1.0-dev libcairo-dev libglib2.0-dev

then build and install:

    git clone https://github.com/amkhlv/hlwinter
    cd hlwinter
    cargo install --path .

This installs two programs: 

1. `wint` for tiling windows

2. `winj` for jumping between windows

They are installed into `~/.cargo/bin/` (which was added to `PATH` when `cargo` was installed).

Use
===

Window tiling
-------------

There is a tiling description file `~/.config/winterreise/tilings.xml`.
Each "geometry" is of the form `x,y,width,height` where `x,y` are the coordinates of the top left corner of the window.
Example of `~/.config/winterreise/tilings.xml`:

    <displays>
      <display resolution="1600x900">
        <window nick="tex" geometry="0,0,930,883"/>
        <window nick="pdf" geometry="850,0,750,890"/>
        <window nick="l" geometry="0,0,800,900"/>
        <window nick="r" geometry="800,0,800,900"/>
      </display>
      <display resolution="1920x1080">
        <window nick="tex" geometry="0,0,1150,1060"/>
        <window nick="pdf" geometry="1100,0,820,1075"/>
        <window nick="l" geometry="0,0,959,1060"/>
        <window nick="r" geometry="960,0,960,1060"/>
      </display>
    </display>

Execution of the command `wint` brings up a dialog window containing:

1. A char-hinted list of windows on the current desktop

2. A command line at the bottom

In the command line, type the description of the desired layout. For example, if window charhinted `a` should be laid out as `tex`,
and window `c` as pdf, type:

    atex cpdf

and press `Enter`. (Notice that the charhint is followed immediately by the name of the tiling model defined in `tilings.xml`.)


Desktop navigation
------------------

Execute:

    winj -h

for help...

The colors of the buttons are determined by the `CSS` file `~/.config/winterreise/style.css`. 
The style classes listed in that file follow the pattern: `wbtn_CLASSNAME`. 
If `CLASSNAME` contains a dot, replace it with underscore:

    org.inkscape.Inkscape -> wbtn_org_inkscape_Inkscape



