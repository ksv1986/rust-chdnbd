# NBD server for MAME CHD files written in Rust

## Usage example

Start server in user console:
```
$ cargo run ./raycris.chd
CHD version: 5
CHD size: 40960000
Compression: LZMA zlib Huffman FLAC
Ratio: 30.3%
Hunk size: 4096
Hunk count: 10000
Hunk compression distribution:
  LZMA: 3277
  zlib: 233
  Huffman: 249
  FLAC: 970
  Uncompressed: 279
  Self: 4992
```

Connect to server from root console:
```
# modprobe nbd
# nbd-client localhost
Warning: the oldstyle protocol is no longer supported.
This method now uses the newstyle protocol with a default export
Negotiation: ..size = 39MB
Connected /dev/nbd0
# parted /dev/nbd0 print
Model: Unknown (unknown)
Disk /dev/nbd0: 41,0MB
Sector size (logical/physical): 512B/512B
Partition Table: msdos
Disk Flags:

Number  Start  End     Size    Type     File system  Flags
 1      8192B  40,9MB  40,9MB  primary  fat16        boot

# mkdir /tmp/nbd
# mount /dev/nbd0p1 /tmp/nbd/
# ls /tmp/nbd/
ATRCT.BIN  E04RR.BIN  E54.BIN     ENE08.BIN  M01WC.BIN  M44.BIN       PRGEND.BIN   SCR2B.BIN   TAITO.TIM
BOSS1.BIN  E05.BIN    E55VR.BIN   ENE17.BIN  M02WJ.BIN  MISS.BIN      PRGRSLT.BIN  SCR2.BIN    T_ASC.BIN
BOSS2.BIN  E09.BIN    E60HS.BIN   ENE20.BIN  M05MH.BIN  PIECE.BIN     PRGSEL.BIN   SCR3.BIN    TIM
BOSS3.BIN  E12HM.BIN  E61WF.BIN   ENE30.BIN  M07.BIN    PRG1.BIN      PRGTM.BIN    SCR4B.BIN   T_NET.BIN
BOSS4.BIN  E14WR.BIN  E99NP.BIN   ENE32.BIN  M09.BIN    PRG2.BIN      R4COM.BIN    SCR4.BIN    T_R9.BIN
BOSS9.BIN  E18RC.BIN  EN06.BIN    ENE35.BIN  M10ZL.BIN  PRG3.BIN      RAY.SDH      SCR5.BIN    VAB
CLUT       E19.BIN    END1.BIN    ENE36.BIN  M17.BIN    PRG4.BIN      RESULT.BIN   SCR6.BIN    W08.BIN
COM.BIN    E38AC.BIN  END2.BIN    ENE39.BIN  M20YP.BIN  PRG5.BIN      SALCAN.BIN   SCR9.BIN    WAVE0.BIN
DELPL.BIN  E50CM.BIN  END3.BIN    ENE51.BIN  M21CR.BIN  PRG6.BIN      SAVEDATA     SELECT.BIN  WAVE1.BIN
E01FR.BIN  E52.BIN    ENDCOM.BIN  ITEMC.BIN  M31RL.BIN  PRG9.BIN      SCR10.BIN    SYSTEM.INF  WAVE2.BIN
E03FW.BIN  E53.BIN    ENE02.BIN   M00.BIN    M32.BIN    PRGATRCT.BIN  SCR1.BIN     SYSTEM.TIM  ZOOM.SDH
#

# umount /tmp/nbd
# nbd-client -d /dev/nbd0
```

## Known limitations
* Write not supported

## License

MIT
