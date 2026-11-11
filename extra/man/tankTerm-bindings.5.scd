TANKTERM-BINDINGS(5)

# NAME

tankTerm-bindings - Default configuration file bindings.

# SYNOPSIS

This page documents all key and mouse bindings for the default TankTerm
configuration. See *tankTerm*(5) for full configuration format documentation.

Every value in these tables is a string, so the first key binding below is
written as { key = _"Paste"_, action = _"Paste"_ } in the configuration file.

# MOUSE BINDINGS

[[ *mouse*
:[ *mods*
:< *action*
|  _Right_
:[
:  _ExpandSelection_
|  _Right_
:  _Control_
:  _ExpandSelection_
|  _Middle_
:  _~Vi_
:  _PasteSelection_

# KEY BINDINGS

[[ *key*
:[ *mods*
:[ *mode*
:< *action* / *chars*
|  _Paste_
:[
:[
:  _Paste_
|  _Copy_
:[
:[
:  _Copy_
|  _L_
:  _Control_
:[
:  _ClearLogNotice_
|  _Paste_
:[
:  _Vi|Search_
:  _Paste_
|  _L_
:  _Control_
:  _~Vi|~Search_
:  *chars*: _"\\u000c"_
|  _PageUp_
:  _Shift_
:  _~Alt_
:  _ScrollPageUp_
|  _PageDown_
:  _Shift_
:  _~Alt_
:  _ScrollPageDown_
|  _Home_
:  _Shift_
:  _~Alt_
:  _ScrollToTop_
|  _End_
:  _Shift_
:  _~Alt_
:  _ScrollToBottom_

## Vi Mode

[[ *key*
:[ *mods*
:[ *mode*
:< *action*
|  _Space_
:  _Shift|Control_
:  _~Search_
:  _ToggleViMode_
|  _Space_
:  _Shift|Control_
:  _Vi|~Search_
:  _ScrollToBottom_
|  _Escape_
:[
:  _Vi|~Search_
:  _ClearSelection_
|  _I_
:[
:  _Vi|~Search_
:  _ToggleViMode_
|  _I_
:[
:  _Vi|~Search_
:  _ScrollToBottom_
|  _C_
:  _Control_
:  _Vi|~Search_
:  _ToggleViMode_
|  _Y_
:  _Control_
:  _Vi|~Search_
:  _ScrollLineUp_
|  _E_
:  _Control_
:  _Vi|~Search_
:  _ScrollLineDown_
|  _G_
:[
:  _Vi|~Search_
:  _ScrollToTop_
|  _G_
:  _Shift_
:  _Vi|~Search_
:  _ScrollToBottom_
|  _B_
:  _Control_
:  _Vi|~Search_
:  _ScrollPageUp_
|  _F_
:  _Control_
:  _Vi|~Search_
:  _ScrollPageDown_
|  _U_
:  _Control_
:  _Vi|~Search_
:  _ScrollHalfPageUp_
|  _D_
:  _Control_
:  _Vi|~Search_
:  _ScrollHalfPageDown_
|  _Y_
:[
:  _Vi|~Search_
:  _Copy_
|  _Y_
:[
:  _Vi|~Search_
:  _ClearSelection_
|  _Copy_
:[
:  _Vi|~Search_
:  _ClearSelection_
|  _Y_
:  _Shift_
:  _Vi|~Search_
:  _ToggleNormalSelection_
|  _Y_
:  _Shift_
:  _Vi|~Search_
:  _Last_
|  _Y_
:  _Shift_
:  _Vi|~Search_
:  _Copy_
|  _Y_
:  _Shift_
:  _Vi|~Search_
:  _ClearSelection_
|  _V_
:[
:  _Vi|~Search_
:  _ToggleNormalSelection_
|  _V_
:  _Shift_
:  _Vi|~Search_
:  _ToggleLineSelection_
|  _V_
:  _Control_
:  _Vi|~Search_
:  _ToggleBlockSelection_
|  _V_
:  _Alt_
:  _Vi|~Search_
:  _ToggleSemanticSelection_
|  _Enter_
:[
:  _Vi|~Search_
:  _Open_
|  _Z_
:[
:  _Vi|~Search_
:  _CenterAroundViCursor_
|  _F_
:[
:  _Vi|~Search_
:  _InlineSearchForward_
|  _F_
:  _Shift_
:  _Vi|~Search_
:  _InlineSearchBackward_
|  _T_
:[
:  _Vi|~Search_
:  _InlineSearchForwardShort_
|  _T_
:  _Shift_
:  _Vi|~Search_
:  _InlineSearchBackwardShort_
|  _;_
:[
:  _Vi|~Search_
:  _InlineSearchNext_
|  _,_
:[
:  _Vi|~Search_
:  _InlineSearchPrevious_
|  _\*_
:  _Shift_
:  _Vi|~Search_
:  _SemanticSearchForward_
|  _#_
:  _Shift_
:  _Vi|~Search_
:  _SemanticSearchBackward_
|  _K_
:[
:  _Vi|~Search_
:  _Up_
|  _J_
:[
:  _Vi|~Search_
:  _Down_
|  _H_
:[
:  _Vi|~Search_
:  _Left_
|  _L_
:[
:  _Vi|~Search_
:  _Right_
|  _ArrowUp_
:[
:  _Vi|~Search_
:  _Up_
|  _ArrowDown_
:[
:  _Vi|~Search_
:  _Down_
|  _ArrowLeft_
:[
:  _Vi|~Search_
:  _Left_
|  _ArrowRight_
:[
:  _Vi|~Search_
:  _Right_
|  _0_
:[
:  _Vi|~Search_
:  _First_
|  _$_
:  _Shift_
:  _Vi|~Search_
:  _Last_
|  _Home_
:[
:  _Vi|~Search_
:  _First_
|  _End_
:[
:  _Vi|~Search_
:  _Last_
|  _^_
:  _Shift_
:  _Vi|~Search_
:  _FirstOccupied_
|  _H_
:  _Shift_
:  _Vi|~Search_
:  _High_
|  _M_
:  _Shift_
:  _Vi|~Search_
:  _Middle_
|  _L_
:  _Shift_
:  _Vi|~Search_
:  _Low_
|  _B_
:[
:  _Vi|~Search_
:  _SemanticLeft_
|  _W_
:[
:  _Vi|~Search_
:  _SemanticRight_
|  _E_
:[
:  _Vi|~Search_
:  _SemanticRightEnd_
|  _B_
:  _Shift_
:  _Vi|~Search_
:  _WordLeft_
|  _W_
:  _Shift_
:  _Vi|~Search_
:  _WordRight_
|  _E_
:  _Shift_
:  _Vi|~Search_
:  _WordRightEnd_
|  _%_
:  _Shift_
:  _Vi|~Search_
:  _Bracket_
|  _{_
:  _Shift_
:  _Vi|~Search_
:  _ParagraphUp_
|  _}_
:  _Shift_
:  _Vi|~Search_
:  _ParagraphDown_
|  _/_
:[
:  _Vi|~Search_
:  _SearchForward_
|  _?_
:  _Shift_
:  _Vi|~Search_
:  _SearchBackward_
|  _N_
:[
:  _Vi|~Search_
:  _SearchNext_
|  _N_
:  _Shift_
:  _Vi|~Search_
:  _SearchPrevious_


## Search Mode

[[ *key*
:[ *mods*
:[ *mode*
:< *action*
|  _Enter_
:[
:  _Search|Vi_
:  _SearchConfirm_
|  _Escape_
:[
:  _Search_
:  _SearchCancel_
|  _C_
:  _Control_
:  _Search_
:  _SearchCancel_
|  _U_
:  _Control_
:  _Search_
:  _SearchClear_
|  _W_
:  _Control_
:  _Search_
:  _SearchDeleteWord_
|  _P_
:  _Control_
:  _Search_
:  _SearchHistoryPrevious_
|  _N_
:  _Control_
:  _Search_
:  _SearchHistoryNext_
|  _ArrowUp_
:[
:  _Search_
:  _SearchHistoryPrevious_
|  _ArrowDown_
:[
:  _Search_
:  _SearchHistoryNext_
|  _Enter_
:[
:  _Search|~Vi_
:  _SearchFocusNext_

## Windows, Linux, and BSD only

[[ *key*
:[ *mods*
:[ *mode*
:< *action*
|  _V_
:  _Control|Shift_
:  _~Vi_
:  _Paste_
|  _V_
:  _Control|Shift_
:  _Vi|Search_
:  _Paste_
|  _C_
:  _Control|Shift_
:[
:  _Copy_
|  _F_
:  _Control|Shift_
:  _~Search_
:  _SearchForward_
|  _B_
:  _Control|Shift_
:  _~Search_
:  _SearchBackward_
|  _C_
:  _Control|Shift_
:  _Vi|~Search_
:  _ClearSelection_
|  _Insert_
:  _Shift_
:[
:  _PasteSelection_
|  _0_
:  _Control_
:[
:  _ResetFontSize_
|  _=_
:  _Control_
:[
:  _IncreaseFontSize_
|  _+_
:  _Control_
:[
:  _IncreaseFontSize_
|  _NumpadAdd_
:  _Control_
:[
:  _IncreaseFontSize_
|  _-_
:  _Control_
:[
:  _DecreaseFontSize_
|  _NumpadSubtract_
:  _Control_
:[
:  _DecreaseFontSize_
|  _H_
:  _Control|Shift_
:[
:  _ToggleHelp_

## Windows only

[[ *key*
:[ *mods*
:[ *mode*
:< *action*
|  _Enter_
:  _Alt_
:[
:  _ToggleFullscreen_

## macOS only

[[ *key*
:[ *mods*
:[ *mode*
:< *action* / *chars*
|  _K_
:  _Command_
:  _~Vi|~Search_
:  *chars*: _"\\u000c"_
|  _K_
:  _Command_
:  _~Vi|~Search_
:  _ClearHistory_
|  _0_
:  _Command_
:[
:  _ResetFontSize_
|  _=_
:  _Command_
:[
:  _IncreaseFontSize_
|  _+_
:  _Command_
:[
:  _IncreaseFontSize_
|  _NumpadAdd_
:  _Command_
:[
:  _IncreaseFontSize_
|  _-_
:  _Command_
:[
:  _DecreaseFontSize_
|  _NumpadSubtract_
:  _Command_
:[
:  _DecreaseFontSize_
|  _V_
:  _Command_
:  _~Vi_
:  _Paste_
|  _V_
:  _Command_
:  _Vi|Search_
:  _Paste_
|  _C_
:  _Command_
:[
:  _Copy_
|  _C_
:  _Command_
:  _Vi|~Search_
:  _ClearSelection_
|  _H_
:  _Command_
:[
:  _Hide_
|  _H_
:  _Command|Alt_
:[
:  _HideOtherApplications_
|  _H_
:  _Command|Shift_
:[
:  _ToggleHelp_
|  _M_
:  _Command_
:[
:  _Minimize_
|  _Q_
:  _Command_
:[
:  _Quit_
|  _N_
:  _Command_
:[
:  _CreateNewWindow_
|  _F_
:  _Command|Control_
:[
:  _ToggleFullscreen_
|  _F_
:  _Command_
:  _~Search_
:  _SearchForward_
|  _B_
:  _Command_
:  _~Search_
:  _SearchBackward_

# TAB AND PANE BINDINGS

The default bindings for tabs and panes depend on the *keyboard.preset* option
of *tankTerm*(5), which is _"Tabby"_ unless it is set.

## Tabby and WezTerm presets on Windows, Linux, and BSD

[[ *key*
:[ *mods*
:< *action*
|  _T_
:  _Control|Shift_
:  _CreateNewTab_
|  _W_
:  _Control|Shift_
:  _CloseTab_
|  _Tab_
:  _Control_
:  _SelectNextTab_
|  _Tab_
:  _Control|Shift_
:  _SelectPreviousTab_
|  _PageUp_
:  _Control|Shift_
:  _MoveTabLeft_
|  _PageDown_
:  _Control|Shift_
:  _MoveTabRight_
|  _S_
:  _Control|Shift_
:  _SplitRight_
|  _D_
:  _Control|Shift_
:  _SplitDown_
|  _W_
:  _Control|Alt_
:  _ClosePane_
|  _Z_
:  _Control|Shift_
:  _TogglePaneZoom_
|  _ArrowLeft_
:  _Control|Shift|Alt_
:  _ResizePaneLeft_
|  _ArrowRight_
:  _Control|Shift|Alt_
:  _ResizePaneRight_
|  _ArrowUp_
:  _Control|Shift|Alt_
:  _ResizePaneUp_
|  _ArrowDown_
:  _Control|Shift|Alt_
:  _ResizePaneDown_
|  _P_
:  _Control|Shift_
:  _ShowCommandPalette_
|  _E_
:  _Control|Shift_
:  _ShowProfiles_

## Tabby preset on Windows, Linux, and BSD

_Control|Shift_ with the arrow keys and with the digits reach the applications
in the terminal. No key is bound to _SelectLastTab_.

[[ *key*
:[ *mods*
:< *action*
|  _1_
:  _Alt_
:  _SelectTab1_
|  _2_
:  _Alt_
:  _SelectTab2_
|  _3_
:  _Alt_
:  _SelectTab3_
|  _4_
:  _Alt_
:  _SelectTab4_
|  _5_
:  _Alt_
:  _SelectTab5_
|  _6_
:  _Alt_
:  _SelectTab6_
|  _7_
:  _Alt_
:  _SelectTab7_
|  _8_
:  _Alt_
:  _SelectTab8_
|  _9_
:  _Alt_
:  _SelectTab9_
|  _ArrowLeft_
:  _Control|Alt_
:  _FocusPaneLeft_
|  _ArrowRight_
:  _Control|Alt_
:  _FocusPaneRight_
|  _ArrowUp_
:  _Control|Alt_
:  _FocusPaneUp_
|  _ArrowDown_
:  _Control|Alt_
:  _FocusPaneDown_

## WezTerm preset on Windows, Linux, and BSD

_Alt_ with the digits and _Control|Alt_ with the arrow keys reach the
applications in the terminal.

[[ *key*
:[ *mods*
:< *action*
|  _1_
:  _Control|Shift_
:  _SelectTab1_
|  _2_
:  _Control|Shift_
:  _SelectTab2_
|  _3_
:  _Control|Shift_
:  _SelectTab3_
|  _4_
:  _Control|Shift_
:  _SelectTab4_
|  _5_
:  _Control|Shift_
:  _SelectTab5_
|  _6_
:  _Control|Shift_
:  _SelectTab6_
|  _7_
:  _Control|Shift_
:  _SelectTab7_
|  _8_
:  _Control|Shift_
:  _SelectTab8_
|  _9_
:  _Control|Shift_
:  _SelectLastTab_
|  _ArrowLeft_
:  _Control|Shift_
:  _FocusPaneLeft_
|  _ArrowRight_
:  _Control|Shift_
:  _FocusPaneRight_
|  _ArrowUp_
:  _Control|Shift_
:  _FocusPaneUp_
|  _ArrowDown_
:  _Control|Shift_
:  _FocusPaneDown_

## Leader preset on Windows, Linux, and BSD

Only these keys run their action directly.

[[ *key*
:[ *mods*
:< *action*
|  _T_
:  _Control|Shift_
:  _CreateNewTab_
|  _S_
:  _Control|Shift_
:  _SplitRight_
|  _D_
:  _Control|Shift_
:  _SplitDown_

## Leader preset: keys after the leader key

These keys are pressed after the leader key, which is _Control+A_ unless
*keyboard.leader* is set. In the configuration file they are bindings with
*mode* = _"Leader"_. They are the same on every platform and only use keys that
need no _Shift_, so they don't depend on the keyboard layout.

[[ *key*
:[ *mods*
:< *action*
|  _C_
:[
:  _CreateNewTab_
|  _K_
:[
:  _CloseTab_
|  _N_
:[
:  _SelectNextTab_
|  _P_
:[
:  _SelectPreviousTab_
|  _1_
:[
:  _SelectTab1_
|  _2_
:[
:  _SelectTab2_
|  _3_
:[
:  _SelectTab3_
|  _4_
:[
:  _SelectTab4_
|  _5_
:[
:  _SelectTab5_
|  _6_
:[
:  _SelectTab6_
|  _7_
:[
:  _SelectTab7_
|  _8_
:[
:  _SelectTab8_
|  _9_
:[
:  _SelectTab9_
|  _0_
:[
:  _SelectLastTab_
|  _,_
:[
:  _MoveTabLeft_
|  _._
:[
:  _MoveTabRight_
|  _\\_
:[
:  _SplitRight_
|  _-_
:[
:  _SplitDown_
|  _X_
:[
:  _ClosePane_
|  _Z_
:[
:  _TogglePaneZoom_
|  _O_
:[
:  _FocusNextPane_
|  _;_
:[
:  _FocusPreviousPane_
|  _ArrowLeft_
:[
:  _FocusPaneLeft_
|  _ArrowRight_
:[
:  _FocusPaneRight_
|  _ArrowUp_
:[
:  _FocusPaneUp_
|  _ArrowDown_
:[
:  _FocusPaneDown_
|  _ArrowLeft_
:  _Control_
:  _ResizePaneLeft_
|  _ArrowRight_
:  _Control_
:  _ResizePaneRight_
|  _ArrowUp_
:  _Control_
:  _ResizePaneUp_
|  _ArrowDown_
:  _Control_
:  _ResizePaneDown_
|  _Space_
:[
:  _ShowCommandPalette_
|  _E_
:[
:  _ShowProfiles_

## Tabby and WezTerm presets on macOS

[[ *key*
:[ *mods*
:< *action*
|  _T_
:  _Command_
:  _CreateNewTab_
|  _W_
:  _Command_
:  _CloseTab_
|  _]_
:  _Command|Shift_
:  _SelectNextTab_
|  _[_
:  _Command|Shift_
:  _SelectPreviousTab_
|  _Tab_
:  _Command_
:  _SelectNextTab_
|  _Tab_
:  _Command|Shift_
:  _SelectPreviousTab_
|  _1_
:  _Command_
:  _SelectTab1_
|  _2_
:  _Command_
:  _SelectTab2_
|  _3_
:  _Command_
:  _SelectTab3_
|  _4_
:  _Command_
:  _SelectTab4_
|  _5_
:  _Command_
:  _SelectTab5_
|  _6_
:  _Command_
:  _SelectTab6_
|  _7_
:  _Command_
:  _SelectTab7_
|  _8_
:  _Command_
:  _SelectTab8_
|  _9_
:  _Command_
:  _SelectLastTab_
|  _D_
:  _Command_
:  _SplitRight_
|  _D_
:  _Command|Shift_
:  _SplitDown_
|  _W_
:  _Command|Shift_
:  _ClosePane_
|  _Enter_
:  _Command|Shift_
:  _TogglePaneZoom_
|  _ArrowLeft_
:  _Command|Alt_
:  _FocusPaneLeft_
|  _ArrowRight_
:  _Command|Alt_
:  _FocusPaneRight_
|  _ArrowUp_
:  _Command|Alt_
:  _FocusPaneUp_
|  _ArrowDown_
:  _Command|Alt_
:  _FocusPaneDown_
|  _P_
:  _Command|Shift_
:  _ShowCommandPalette_
|  _E_
:  _Command|Shift_
:  _ShowProfiles_

## Leader preset on macOS

Only these keys run their action directly; the keys after the leader key are
listed above.

[[ *key*
:[ *mods*
:< *action*
|  _T_
:  _Command_
:  _CreateNewTab_
|  _D_
:  _Command_
:  _SplitRight_
|  _D_
:  _Command|Shift_
:  _SplitDown_

# SEE ALSO

*tankTerm*(1), *tankTerm-msg*(1), *tankTerm*(5), *tankTerm-escapes*(7)

# BUGS

Found a bug? Please report it at _https://git.infra.xitely.net/xitely/TankTerm/issues_.

# MAINTAINERS

- Christian Duerr <contact@christianduerr.com>
- Kirill Chibisov <contact@kchibisov.com>
