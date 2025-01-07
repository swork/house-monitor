target extended-remote /dev/cu.usbmodem81D563B31

# print demangled symbols
set print asm-demangle on

# set backtrace limit to not have infinite backtrace loops
set backtrace limit 32

mon a
