# Odyssey [![Discord Link](https://discordapp.com/api/guilds/881628699500359731/widget.png?style=shield)](https://discord.gg/GFUn9gwRsj)
[![GitHub license](https://img.shields.io/github/license/TheContrappostoShop/Odyssey.svg?style=for-the-badge)](https://github.com/TheContrappostoShop/Odyssey/blob/main/LICENSE)
[![GitHub release](https://img.shields.io/github/release/TheContrappostoShop/Odyssey.svg?style=for-the-badge)](https://github.com/TheContrappostoShop/Odyssey/releases)

Engine for processing and printing Prusa SL1 slicer files, designed for the
[Apollo](https://github.com/TheContrappostoShop/Apollo) series of control board
and the
[Prometheus MSLA](https://github.com/TheContrappostoShop/Prometheus-MSLA) Open
Source Resin 3D Printer.

> :warning: **This project is a work in progress. Exercise caution when using
it for the first time, and don't print unattended.**

## How To Use Odyssey

### Direct API Calls
After [installing](#installation) Odyssey, and running the program (ideally as
a service), you can begin interacting with it via the REST API, running on port
12357 by default. HTTP requests can be made with your tool of choice (such as
cURL), or using the provided `apiHelper.py` python script. In addition to a
simpler command line interface, with script also provides some easy to follow
documentation of the available endpoints. See below for further details.

Example Usage:
```
./apiHelper.py start Local sliced_model.sl1
```

Command help:
```
usage: apiHelper.py [-h] [-u URL]  ...

This script provides an easier way to interact with the Odyssey API from a local context, such as Klipper Macros or the command line.

optional arguments:
  -h, --help         show this help message and exit
  -u URL, --url URL

API Endpoints:
  Valid CLI Endpoints

  
    start            Start printing the specified file
    cancel           cancel the current print (at the end of the current layer)
    pause            Pause the current print (at the end of the current layer)
    resume           Resume a previously paused print
    status           Return the current status from Odyssey
    manual_control   Move the z axis of the printer, or toggle curing
```

### Mainsail Integration
While work on Orion continues, we have implemented a temporary integration with
the well-establish Mainsail UI for Klipper. This provides an easy-to-use web
interface, accessible by typing your RPi's IP address into your favorite web
browser's address bar. For more information, see
[Mainsail's documentation](https://docs.mainsail.xyz/).

The Odyssey integration hijacks the normal pause/resume/stop functionality of
Mainsail, as well as allowing you to start prints directly from the web ui.

When you upload a `.sl1` file (or other supported file type), a fake `.gcode`
file is generated to represent that print. When you select that file in Mainsail,
that file request is intercepted, and Odyssey is told to begin printing the
corresponding `.sl` file. 

With the complexity of the Mainsail integration, and all of the services and
files involved, it is *highly* suggested that you follow the
[easy install](#easy-install) process and use our prebuilt OS image. 

### Orion UI
> Coming Soon!

## Installation

### Easy Install

For a fully configured Raspberry Pi Installation of Odyssey, Klipper, and
Mainsail, see the
[PrometheusOS](https://github.com/TheContrappostoShop/PrometheusOS#odyssey-variant)
custom RPi image.

### Manual Install

If you do not wish to use the custom RPi image, you can set up
[Klipper](https://www.klipper3d.org/) and Odyssey yourself.

To install Odyssey on your Raspberry Pi, simply download the latest release
[here](https://github.com/TheContrappostoShop/Odyssey/releases) and unpack it
on your device. The provided tar.gz archive will contain the Odyssey binary, the
default odyssey.yaml configuration file, and an API helper script. For more
information on how to configure Odyssey, see [Configuraion](#configuration)
below.

For the Klipper installation, see the
[Prometheus Config](https://github.com/TheContrappostoShop/Prometheus_Config/#manual-install)
repo's instructions on manual installation.

## Configuration
Odyssey relies on a .yaml configuration file, specified at runtime, to know how
to interact with your printer's hardware, and run the appropriate gcode commands
for a successful print. An example configuration, set up for the Prometheus-MSLA,
is provided [here](odyssey.yaml), and further information about each of the
fields is listed below:

### printer
This section holds the config fields related to the printer, such as the path to
your machine's serial port, and the specifications of your display's frame
buffer.

#### serial
This is the path to your control board's serial port, used by Odyssey for sending
Gcode commands and receiving responses.

For Klipper, this is a virtual serial port which interface with Klippy, found
at ` /home/pi/printer_data/comms/klippy.serial`, for a normal Prometheus-MSLA
installation, or at `/tmp/printer` for a default Klipper install.

#### baudrate
This is the baudrate of the serial port specified above. For a direct connection
to a physical control board, this value may vary, but if you're using klipper
then is should always be `250000`.

#### frame_buffer
This is the path to the frame buffer device representing your machine's LCD
display. Typically, this will be either `/dev/fb0` or `/dev/fb1`.

#### fb_bit_depth
This is the bit depth of your display, or how many bits go into each pixel on
the screen. This can vary depending on your specific hardware, but for many
monochrome printer screens will be `5`. See [fb_chunk_size](#fb_chunk_size) for
more information.

#### fb_chunk_size
This field represents the size of a "group" of pixels on your display, with
a common value being 16 bits, or 2 bytes.

For a normal RGB555 screen, a single "group" would only ever contain just a
single pixel, and the 16 bits would be divided between the Red, Green, and Blue
channels, each getting 5 and the remaining bit serving as a spacer between
values.

Many Monochrome screens, however, misuse the RGB555 bit layout to represent a
grouping of three separate monochrome pixels, one for each color channel.

For screens that do this sort of grouping, Odyssey needs to know how many bits each
grouping is, so it can properly divide it by the configured bit depth, and use
any remaining bits in the group as spacers.

#### max_z
This is the max z position for your machine. This value can be accessed in the
[gcode](#gcode) configuration segments with the substitution `{z_max}`.

#### z_lift
This field specifies how far up to raise the build plate after a layer is cured,
before lowering it back down to cure the next layer.

### gcode
This section holds fields pertaining to the Gcode used to drive the machine's
hardware and signal between the board and Odyssey.

#### boot
This field will be sent to the printer when Odyssey first starts up, and can
be used for anything that only needs to be set once (such as Absolute
Positioning with `G90`).

#### shutdown
This field will be sent to the printer when Odyssey is shutting down, and
should be used to bring the printer to a safe low-power state. Common commands
for this field are `M84` to power down the motors, and your
[cure_end](#cure_end) command to ensure the LED array is powered off.

#### home_command
This field will be used to home the printer's kinematics before the start of a
new print, or when requested via the API interface (Coming soon!). The most
common case is just `G28`, but it is exposed here to allow for more complicated
scenarios.

#### move_command
This field is used to move the printer to a specific position, given by the
substitution `{z}`.

#### print_start
This field will be sent to the printer at the start of each print, and can be
used for things like enabling fans, or turning on chamber LEDs.

#### print_end
This field will be sent to the printer at the end of each print, and should be
used to disable fans/lights if needed, and raise the build plate for easier
removal.

#### cure_start
This field will be used to start curing each given layer, and should contain the
gcode necessary to enable your LED array (or similar hardware). It is
recommended that you test this command before running an actual print.

#### cure_end
This field will be used to stop curing each given layer, and should contain the
gcode necessary to disable your LED array (or similar hardware). It is
recommended that you test this command before running an actual print.

#### sync_message
This field represents the response value that Odyssey will look for after
executing a move, to ensure that the move has been completed before printing
continues. For printer firmwares that support NanoDLP, the response message is
usually `Z_move_comp`.

See the
[Prometheus Klipper Config](https://github.com/TheContrappostoShop/Prometheus_Config/blob/6d7de4b9e4ba00d209c34f0592ec65d28a77a26e/klipper/config/printer.cfg#L117)
for an example of how to implement this functionality on the firmware side.
