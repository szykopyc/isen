" Vim syntax file
" Language: Isen
" The @@ is not a typo.

if exists("b:current_syntax")
  finish
endif

syn case match

syn keyword isenDeclaration dec
syn keyword isenBorrow borrow from
syn keyword isenShare share
syn keyword isenRoutine given
syn keyword isenReturn ret
syn keyword isenExit exit
syn keyword isenControl if else aslongas each in enough onwards attempt recover always
syn keyword isenStructure space form problem
syn keyword isenOutput say
syn keyword isenWarning shout
syn keyword isenException scream
syn keyword isenType int int64 float bool string json naught unit perchance udp_socket udp_packet tcp_listener tcp_stream http_response list arr map Problem
syn keyword isenBoolean true false

syn match isenTypeMarker /@@/
syn match isenBlockDelimiter /\$/
syn match isenBlockDelimiter /\\\$/
syn match isenMapDelimiter /#\ze{/
syn match isenFunctionName /\<given\s\+\zs[A-Za-z_][A-Za-z0-9_]*/
syn match isenFormName /\<\%(form\|problem\)\s\+\zs[A-Z][A-Za-z0-9_]*/
syn match isenNamespace /\<space\s\+\zs[A-Z][A-Za-z0-9_]*/
syn match isenBuiltin /\<\%(Time\|Random\|Args\|Kwargs\|Env\|Path\|Json\|Test\|Bytes\|Udp\|Tcp\|Http\|String\|Array\|Maths\|Ordering\|File\|List\|Map\|Stack\|Queue\|Range\|Input\|Keyboard\|LengText\|size\)\ze\%($\|\.\)/
syn match isenCast /\.pour_into\ze(/
syn match isenField /\.[A-Za-z_][A-Za-z0-9_]*/
syn match isenNumber /\<\d\+\%(.\d\+\)\?\>/

syn region isenString start=/"/ skip=/\\./ end=/"/
syn match isenComment /\/\/.*$/
syn match isenComment /#\%({\)\@!.*$/
syn match isenDocComment /\/\/\/.*$/

hi def link isenDeclaration Keyword
hi def link isenBorrow Include
hi def link isenShare Keyword
hi def link isenRoutine Keyword
hi def link isenReturn Keyword
hi def link isenExit Keyword
hi def link isenControl Conditional
hi def link isenStructure Keyword
hi def link isenOutput Statement
hi def link isenWarning WarningMsg
hi def link isenException Error
hi def link isenType Type
hi def link isenBoolean Boolean
hi def link isenTypeMarker Special
hi def link isenBlockDelimiter Delimiter
hi def link isenMapDelimiter Delimiter
hi def link isenFunctionName Function
hi def link isenFormName Type
hi def link isenNamespace Namespace
hi def link isenBuiltin PreProc
hi def link isenCast Special
hi def link isenField Identifier
hi def link isenNumber Number
hi def link isenString String
hi def link isenComment Comment
hi def link isenDocComment SpecialComment

let b:current_syntax = "isen"
