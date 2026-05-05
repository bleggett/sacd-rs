# SACD ripper utility

Designed to be used with a jailbroken SACD player, such as specific models of PS3s or some Sony Bluray players,
which can support running a server to stream raw SACD image data over the network to a client.

Heavily based on https://github.com/EuFlo/sacd-ripper and others

This currently implements an `sacd_extract`-style client and [most of the Scarletbook spec](https://archive.org/details/super-audio-cd-system-description/SACDspecP2audio_200%20contents/), and can:

- Stream the entire contents of the SACD layer from a jailbroken player over the network to a local decrypted ISO.
- Extract .DSF files from the multichannel or stereo layer of a local SACD ISO
- Handle SACDs with DST-encoded DSF files with a full DST decoder for 1-bit PDM.
- Extract SACD metadata, text, album info, ISRC codes, etc from the SACD manifest in an `sacd_extract`-style format.

In short, more or less everything the `sacd_extract` client from https://github.com/EuFlo/sacd-ripper supports and does.

Notable exceptions are 
- Cue sheet generation

I intend to add new features that `sacd_extract` lacks and which I would find personally useful, such as an easier way to submit ISRC codes to MusicBrainz, etc.

# Installation

1. Ensure you have the `protobuf` compiler binary in $PATH:

``` sh
which protoc
```

if not, install it via your package manager.

1. Install the CLI

```
cargo install sacd-rs
```

# Usage

The following networked examples assume you have a jailbroken SACD player residing on your LAN at 192.168.1.222.

Jailbreaking players requires that you download a specific, not-currently-open binary and load it onto a USB stick. Setting that up is outside the scope of this repo. Many Sony players are supported, check [this forum for details.](https://hifihaven.org/index.php?threads%2Frip-sacd-with-a-blu-ray-player.3652%2F)

Each command should have help and option listings, for example:

``` sh
$ sacd-rs

SACD extraction utility

Usage: sacd-rs <COMMAND>

Commands:
  dump-iso     Dump an ISO image from a network SACD server
  print-info   Print disc and track information
  extract      Extract DSF files from an SACD ISO image
  extract-net  Extract DSF files directly from a network SACD server (no ISO needed)
  help         Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

## dump-iso

This accepts a LAN IP, and assuming the jailbroken SACD player is active, will stream the entire (decrypted) SACD layer contents of the disc in that player over the network to the given target directory as an `.ISO`. The ISO will be decrypted but otherwise untouched, including DST encoding and disc metadata.


## print-info

This accepts a LAN IP, and assuming the jailbroken SACD player is active, will stream the TOCs and track/layer info of the disc in that player over the network.


## extract

This accepts a local path to a previously (via `dump-iso`) extracted and decrypted SACD ISO file, and will extract either the stereo layer, or multichannel layer, or both layer (default) tracks as .DSF files, doing DST decompression if the disc is DST-compressed.

## extract-net

Same, but extracts to local .DSF files over the network directly, skipping the ISO dump step.
