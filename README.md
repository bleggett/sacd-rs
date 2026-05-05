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
