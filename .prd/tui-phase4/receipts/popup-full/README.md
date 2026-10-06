# Popup opened and closed on a full screen (C3 follow-up)

C3 reported, as a suspected defect, that after the slash popup closes on a screen the document had filled, the next notice lands with blank rows under the band. Scenario `scripts/fxcap/scenarios/popup-full.cap` at 24x80, run on both binaries:

- `full`, `closed`, `after`: rows 2-24 identical between fx c1db919 and xfx (version string masked).
- `popup`: identical except fx's 36 commands vs xfx's 10 and `/clear`'s summary.

fx does the same thing: the popup's rows carry the document into scrollback, and when it closes the footer sits under what is left on the screen, with blank rows below it until the document reaches the bottom again. Not a defect.
