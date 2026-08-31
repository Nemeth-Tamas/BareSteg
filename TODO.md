# BareSteg TODO

BareSteg is a deliberately bare, standard-library-only Rust project for hiding
arbitrary data inside image content.

The project is not intended to use conventional LSB steganography or append
payload data to the image file.

The long-term goal is a custom, resilient image carrier that can survive
real-world image processing such as Facebook Messenger recompression and
resizing while remaining decodable by BareSteg.

## Project Rules

- [x] Project name: BareSteg
- [x] GitHub repository: `Nemeth-Tamas/BareSteg`
- [x] Development branch: `main`
- [x] Rust standard library only
- [x] No external crates
- [x] Multi-file source layout from the beginning
- [x] `TODO.md` must be updated on every single development edit
- [x] Every development slice must be committed and pushed to GitHub
- [x] Use `git add .`
- [x] Do not use routine `git status`, `git log`, or similar inspection commands
- [x] Code edits after bootstrap must use exact FIND -> REPLACE blocks
- [x] FIND blocks must be copied from the current GitHub source
- [x] Preserve exact indentation in FIND -> REPLACE edits
- [x] The first real development commit after project creation must already
      contain the working core of the steganography mechanism
- [x] Do not postpone the core mechanism behind placeholder functions

## Current Phase

### Milestone 1 - BMP Roundtrip Proof of Concept

- [x] Create Cargo project
- [x] Create initial multi-file source layout
- [x] Commit and push the initial project creation to `main`
- [x] Verify all formatting, tests, and Clippy gates
- [x] Verify an end-to-end BMP hide/reveal roundtrip
- [x] Commit and push the first working proof of concept
- [x] Confirm raw relative-luminance carrier transport works

The first BareSteg proof of concept successfully hides and independently
recovers arbitrary payload bytes from a 24-bit BI_RGB BMP with CRC32
verification.

The first carrier version proved the transport but produced a clearly visible
band of modified cells because logical cells were allocated sequentially and
every cell was forced to a luminance difference of 40.

### Completed Slice - Distributed Adaptive Carrier

- [x] Replace sequential carrier allocation with deterministic whole-image
      cell permutation
- [x] Replace fixed luminance strength with content-adaptive modulation
- [x] Reduce minimum carrier luminance difference substantially
- [x] Preserve unique one-to-one physical cell allocation
- [x] Verify exact payload roundtrip after distributed carrier change
- [x] Visually compare the new carrier against the original image
- [x] Commit and push distributed adaptive carrier implementation

The distributed carrier removed the obvious contiguous payload band and made
the hidden data substantially less visible.

A same-resolution JPEG quality-30 stress test of the uploaded carrier recovered
all 536 encoded frame bits without error, showing substantial compression
margin in the current raw carrier.

### Current Slice - Bounded Quantization Carrier

- [x] Replace sign-forcing carrier modulation with quantization-index
      modulation
- [x] Bound required luminance-difference movement to one quantization step
- [x] Preserve deterministic whole-image cell permutation
- [x] Add exhaustive quantization target tests
- [x] Verify exact payload roundtrip with quantized carrier
- [x] Visually compare quantized carrier against previous carrier
- [x] Commit and push quantized carrier implementation

The QIM carrier encodes bits using alternating luminance-difference buckets
rather than requiring a particular positive or negative sign.

With a quantization step of 20, embedding never needs to move a cell's
left-versus-right luminance difference by more than 20 levels. In ordinary
unsaturated cells this corresponds to no more than roughly 10 levels of
per-half pixel adjustment.

This should substantially reduce visible changes in naturally asymmetric
cells while retaining useful tolerance against lossy recompression.

Messenger resizing resilience remains unresolved and is a later carrier-layout
problem rather than a reason to increase raw modulation strength.

## Planned Initial Source Layout

- [x] `src/main.rs`
  - CLI argument handling
  - command routing
  - user-facing errors

- [x] `src/bmp.rs`
  - read BMP files
  - validate supported BMP format
  - expose image dimensions
  - expose pixel access
  - preserve/write valid BMP files

- [x] `src/carrier.rs`
  - divide image into logical carrier regions
  - embed encoded bits into image content
  - recover encoded bits from image content
  - avoid dependency on exact original pixel values

- [x] `src/frame.rs`
  - BareSteg payload framing
  - format/version marker
  - payload length
  - payload bytes
  - integrity metadata

- [x] `src/crc32.rs`
  - standard-library-only CRC32 implementation
  - payload corruption detection

- [x] `src/ecc.rs`
  - reserved for redundancy/error-correction work after the first POC

## Milestone 1 - BMP Roundtrip Proof of Concept

Goal:

Hide an arbitrary binary file inside a supported BMP image, write the modified
image to disk, then recover the payload from that image in a separate BareSteg
invocation.

### Required CLI

- [x] `baresteg hide <carrier.bmp> <payload> <output.bmp>`
- [x] `baresteg reveal <image.bmp> <output>`

### BMP Support

Initial scope:

- [ ] Windows BMP signature validation
- [ ] BITMAPFILEHEADER parsing
- [ ] BITMAPINFOHEADER parsing
- [ ] uncompressed BI_RGB images
- [ ] 24-bit RGB/BGR pixel data
- [ ] row padding support
- [ ] bottom-up BMP support
- [ ] reject unsupported BMP variants cleanly

### BareSteg Frame v0

Initial payload frame should contain enough information for independent
recovery.

Planned fields:

- [ ] BareSteg synchronization/magic pattern
- [ ] format version
- [ ] payload length
- [ ] payload CRC32
- [ ] payload bytes

Do not treat this frame format as frozen until resilience testing begins.

### Initial Carrier Algorithm

Do NOT use simple pixel LSB replacement.

The first POC should encode data using visible-pixel properties that have a
chance of surviving later lossy image processing.

Initial direction:

- [x] divide usable image area into logical cells
- [x] encode one logical bit using a relative luminance relationship inside
      each cell
- [x] encode `0` and `1` using opposite luminance relationships
- [x] decode based on relative measurements rather than exact RGB values
- [x] keep modifications bounded to valid channel values
- [ ] reserve room around image boundaries if useful for later resizing
      tolerance

The exact cell geometry and modulation strength are experimental.

### POC Definition of Done

Milestone 1 is NOT complete merely because pixels can be modified.

All of these must work:

- [ ] arbitrary payload bytes can be loaded
- [ ] BareSteg frame can be generated
- [ ] carrier capacity is checked before modification
- [ ] payload can be embedded into a BMP
- [ ] modified BMP can be written
- [ ] modified BMP can be reopened from disk
- [ ] BareSteg frame can be detected
- [ ] payload length can be recovered
- [ ] payload bytes can be recovered
- [ ] CRC32 validates the recovered payload
- [ ] recovered file is byte-for-byte identical to the original payload
- [ ] malformed images fail cleanly
- [ ] images without BareSteg data fail cleanly
- [ ] oversized payloads fail cleanly

## Milestone 2 - Carrier Robustness

After exact BMP roundtrip works:

- [x] add repeated carrier bits
- [x] majority-vote repeated bits
- [x] interleave repeated bits across distant image regions
- [x] protect header more heavily than payload
- [x] recover from isolated damaged carrier cells
- [x] expose useful corruption/recovery diagnostics

### Synchronization

- [ ] investigate repeated synchronization markers
- [x] investigate normalized image-relative carrier positions
- [ ] tolerate small dimension changes
- [ ] tolerate image cropping where practical
- [ ] tolerate moderate luminance/color shifts

## Milestone 3 - Error Correction

Start simple and measure before implementing anything huge.

- [x] characterize actual carrier bit errors
- [x] implement repetition coding
- [x] implement interleaving
- [ ] evaluate Hamming-style coding
- [ ] evaluate convolutional coding
- [ ] consider stronger ECC only if real testing proves it necessary

No external ECC crate is permitted.

## Milestone 4 - Messenger Torture Testing

The target is real Messenger behavior, not theoretical JPEG resilience.

Create a repeatable manual corpus:

- [ ] original carrier image
- [ ] BareSteg output image
- [ ] image uploaded through Messenger
- [ ] downloaded Messenger result
- [x] BareSteg decode result
- [x] recovered payload comparison
- [x] record image dimensions before/after
- [ ] record file format before/after
- [ ] record file size before/after
- [ ] record carrier bit error rate where measurable

Test:

- [ ] normal Messenger photo upload
- [ ] Messenger HD photo upload
- [ ] different source resolutions
- [ ] portrait images
- [ ] landscape images
- [ ] highly detailed images
- [ ] smooth/low-detail images
- [ ] screenshots
- [ ] repeated upload/download generations

## Milestone 5 - Resilience Profiles

Potential CLI profiles:

- [ ] `subtle`
- [ ] `normal`
- [ ] `tank`

Profiles may vary:

- [ ] carrier cell size
- [ ] luminance modulation strength
- [ ] bit repetition count
- [ ] interleaving distance
- [ ] synchronization redundancy
- [ ] ECC strength
- [ ] usable capacity

The decoder should recover profile information from the encoded image rather
than requiring the user to remember it.

## Milestone 6 - JPEG Input

Messenger resilience will eventually require decoding the image format that
Messenger actually returns.

- [ ] gather real Messenger output samples first
- [ ] document the JPEG features used by those samples
- [ ] implement only the required JPEG decoder subset initially
- [ ] JPEG marker parsing
- [ ] quantization tables
- [ ] Huffman tables
- [ ] baseline DCT decoding
- [ ] dequantization
- [ ] inverse DCT
- [ ] YCbCr conversion
- [ ] chroma subsampling support as required
- [ ] reconstruct pixels sufficiently accurately for BareSteg decoding

Do not attempt to write an entire general-purpose image library unless the
real input corpus requires it.

## Milestone 7 - Capacity and Diagnostics

- [ ] capacity calculator
- [ ] report raw carrier bits available
- [ ] report protected payload capacity
- [ ] report payload overhead
- [ ] report detected image dimensions
- [ ] report detected BareSteg format version
- [ ] report CRC result
- [ ] report carrier confidence
- [ ] report corrected/rejected bits where possible

Potential command:

- [ ] `baresteg inspect <image>`

## Milestone 8 - Custom Carrier Layout

BareSteg should not resemble a conventional steganography format.

Potential experiments:

- [ ] deterministic pseudo-random cell traversal
- [ ] image-dimension-derived traversal
- [ ] payload-length-dependent layout
- [ ] multiple separated carrier planes
- [ ] custom synchronization sequence
- [ ] versioned carrier layout
- [ ] image-content-aware cell selection

This provides format uniqueness, not cryptographic secrecy.

Do not claim BareSteg is cryptographically secure merely because other tools
do not recognize its carrier format.

## Milestone 9 - Optional Payload Metadata

After the transport is reliable:

- [ ] optional original filename
- [ ] optional extension
- [ ] optional payload timestamp
- [ ] metadata length limits
- [ ] filenames treated as untrusted data on extraction
- [ ] never allow extracted metadata to escape the requested output path

## Milestone 10 - Additional Image Formats

Only after BMP and Messenger/JPEG requirements are understood.

Possible future formats:

- [ ] PNG input
- [ ] PNG output
- [ ] JPEG input
- [ ] JPEG output
- [ ] investigate WebP only if Messenger testing makes it relevant

## Testing

Every implemented module should gain tests as practical.

- [ ] CRC32 known-vector tests
- [ ] frame encode/decode tests
- [ ] malformed frame tests
- [ ] BMP header parsing tests
- [ ] BMP row-padding tests
- [ ] carrier bit roundtrip tests
- [ ] payload roundtrip tests
- [ ] insufficient-capacity tests
- [ ] corruption tests
- [ ] real Messenger regression corpus

## Development Gates

After each development slice:

```text
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings