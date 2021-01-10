// LZMA SDK helper to minimize ffi calls

#include <LzmaEnc.h>
#include <LzmaDec.h>
#include <Alloc.h>

static ISzAllocPtr allocator = &g_Alloc;

size_t lzma_create(UInt32 hunkbytes)
{
    CLzmaDec *dec = MyAlloc(sizeof(*dec));
    if (!dec)
        return 0;
    LzmaDec_Construct(dec);

    // FIXME: this code is written in a way that makes it impossible to safely upgrade the LZMA SDK
    // This code assumes that the current version of the encoder imposes the same requirements on the
    // decoder as the encoder used to produce the file.  This is not necessarily true.  The format
    // needs to be changed so the encoder properties are written to the file.

    // configure the properties like the compressor did
    CLzmaEncProps encoder_props;
    LzmaEncProps_Init(&encoder_props);
    encoder_props.level = 9;
    encoder_props.reduceSize = hunkbytes;
    LzmaEncProps_Normalize(&encoder_props);

    // convert to decoder properties
    CLzmaEncHandle enc = LzmaEnc_Create(allocator);
    if (!enc)
        goto fail;

    if (LzmaEnc_SetProps(enc, &encoder_props) != SZ_OK) {
        LzmaEnc_Destroy(enc, allocator, allocator);
        goto fail;
    }

    Byte decoder_props[LZMA_PROPS_SIZE];
    SizeT props_size = sizeof(decoder_props);
    SRes res = LzmaEnc_WriteProperties(enc, decoder_props, &props_size);
    LzmaEnc_Destroy(enc, allocator, allocator);
    if (res != SZ_OK)
        goto fail;

    // do memory allocations
    if (LzmaDec_Allocate(dec, decoder_props, LZMA_PROPS_SIZE, allocator) != SZ_OK)
        goto fail;

    return (size_t)dec;

fail:
    MyFree(dec);
    return 0;
}

void lzma_destroy(size_t _dec)
{
    CLzmaDec *dec = (void*)_dec;
    if (!dec)
        return;
    LzmaDec_Free(dec, allocator);
    MyFree((void*)dec);
}

int lzma_decompress(size_t _dec, const Byte *src, UInt32 complen, Byte *dest, UInt32 destlen)
{
    CLzmaDec *dec = (void*)_dec;
    // initialize
    LzmaDec_Init(dec);

    // decode
    SizeT consumedlen = complen;
    SizeT decodedlen = destlen;
    ELzmaStatus status;
    SRes res = LzmaDec_DecodeToBuf(dec, dest, &decodedlen, src, &consumedlen, LZMA_FINISH_END, &status);
    return ((res != SZ_OK && res != LZMA_STATUS_MAYBE_FINISHED_WITHOUT_MARK) || consumedlen != complen || decodedlen != destlen);
}
