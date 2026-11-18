import struct
import sys

FAMILY_NRF52840 = 0xADA52840
PAYLOAD = 256


def blocks(image, base):
    chunks = [image[i:i + PAYLOAD] for i in range(0, len(image), PAYLOAD)]
    for number, chunk in enumerate(chunks):
        header = struct.pack(
            "<8I", 0x0A324655, 0x9E5D5157, 0x2000, base + number * PAYLOAD,
            len(chunk), number, len(chunks), FAMILY_NRF52840,
        )
        yield header + chunk.ljust(476, b"\0") + struct.pack("<I", 0x0AB16F30)


def main(binary, base, output):
    with open(binary, "rb") as source:
        image = source.read()
    with open(output, "wb") as target:
        target.writelines(blocks(image, int(base, 0)))


if __name__ == "__main__":
    main(*sys.argv[1:])
