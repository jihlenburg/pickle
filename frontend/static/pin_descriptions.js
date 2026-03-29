// Pin function descriptions and grouping — pattern-based, device-family agnostic.
// Used for tooltips and dropdown organization.

// Pattern-based description generator. Works for any Microchip PIC/dsPIC device
// without needing per-pin entries.
const _DESC_PATTERNS = [
    // GPIO
    [/^R([A-Z])(\d+)$/, (m) => `Port ${m[1]} bit ${m[2]} (in/out)`],
    [/^RP(\d+)$/, (m) => `Remappable pin ${m[1]}`],

    // Power
    [/^VDD/, () => 'Positive supply'],
    [/^VSS/, () => 'Ground'],
    [/^AVDD/, () => 'Analog positive supply'],
    [/^AVSS/, () => 'Analog ground'],
    [/^MCLR$/, () => 'Master clear (active-low reset) / VPP programming voltage'],
    [/^VREF\+$/, () => 'ADC positive voltage reference'],
    [/^VREF-$/, () => 'ADC negative voltage reference'],

    // ADC
    [/^ANA(\d+)$/, (m) => `Dedicated ADC core ${m[1]} (input)`],
    [/^AN(\d+)$/, (m) => `Shared ADC channel ${m[1]} (input)`],
    [/^ADCTRG(\d+)$/, (m) => `ADC trigger ${m[1]} (external input)`],

    // Op-Amp
    [/^OA(\d+)OUT$/, (m) => `Op-Amp ${m[1]} (output)`],
    [/^OA(\d+)IN\+$/, (m) => `Op-Amp ${m[1]} non-inverting (input)`],
    [/^OA(\d+)IN-$/, (m) => `Op-Amp ${m[1]} inverting (input)`],

    // Comparator
    [/^CMP(\d+)([A-D])$/, (m) => `Comparator ${m[1]} input ${m[2]} (fixed)`],
    [/^CMP(\d+)$/, (m) => `Comparator ${m[1]} (output, remappable)`],

    // DAC
    [/^DAC(\d*)OUT$/, (m) => `DAC${m[1] || ''} (output, buffered)`],
    [/^IBIAS(\d+)$/, (m) => `Internal bias current ${m[1]} (DAC/comparator)`],

    // Oscillator
    [/^OSCI$/, () => 'Oscillator crystal (input)'],
    [/^OSCO$/, () => 'Oscillator crystal (output) / CLKO'],
    [/^CLKI$/, () => 'External clock (input)'],
    [/^CLKO$/, () => 'System clock (output, Fosc/2)'],

    // UART
    [/^U(\d+)TX$/, (m) => `UART${m[1]} transmit (output)`],
    [/^U(\d+)RX$/, (m) => `UART${m[1]} receive (input)`],
    [/^U(\d+)CTS$/, (m) => `UART${m[1]} clear to send (flow control input)`],
    [/^U(\d+)RTS$/, (m) => `UART${m[1]} request to send (flow control output)`],
    [/^U(\d+)DTR$/, (m) => `UART${m[1]} data terminal ready (flow control output)`],
    [/^U(\d+)DSR$/, (m) => `UART${m[1]} data set ready (flow control input)`],

    // SPI
    [/^SDI(\d+)$/, (m) => `SPI${m[1]} data in (MISO in master mode)`],
    [/^SDO(\d+)$/, (m) => `SPI${m[1]} data out (MOSI in master mode)`],
    [/^SCK(\d+)IN$/, (m) => `SPI${m[1]} clock input (slave mode)`],
    [/^SCK(\d+)OUT$/, (m) => `SPI${m[1]} clock output (master mode)`],
    [/^SS(\d+)OUT$/, (m) => `SPI${m[1]} slave select (output)`],
    [/^SS(\d+)$/, (m) => `SPI${m[1]} slave select (input)`],

    // I2C
    [/^SCL(\d+)$/, (m) => `I2C${m[1]} clock (fixed, open-drain)`],
    [/^SDA(\d+)$/, (m) => `I2C${m[1]} data (fixed, open-drain)`],

    // High-Speed PWM
    [/^PWM(\d+)H$/, (m) => `PWM generator ${m[1]} high-side (output)`],
    [/^PWM(\d+)L$/, (m) => `PWM generator ${m[1]} low-side (output)`],
    [/^PWME([A-Z])$/, (m) => `PWM event output ${m[1]}`],
    [/^PWMTRG(\d+)$/, (m) => `PWM trigger ${m[1]} (input)`],

    // PWM Fault / PCI
    [/^PCI(\d+)$/, (m) => `PWM combinational input ${m[1]} (fault/current limit)`],

    // SCCP / MCCP
    [/^OCM(\d+)([A-F])$/, (m) => `CCP${m[1]} output ${m[2]} (PWM/output compare)`],
    [/^OCF([A-Z])$/, (m) => `Output compare fault ${m[1]} (input)`],

    // Input Capture
    [/^ICM(\d+)$/, (m) => `Input capture module ${m[1]} (input)`],

    // Timer
    [/^T(\d+)CK$/, (m) => `Timer ${m[1]} external clock (input)`],
    [/^TCKI(\d+)$/, (m) => `CCP${m[1]} timer external clock (input)`],
    [/^TMR(\d+)$/, (m) => `Timer ${m[1]}`],

    // QEI
    [/^QEA(\d+)$/, (m) => `QEI${m[1]} phase A (input)`],
    [/^QEB(\d+)$/, (m) => `QEI${m[1]} phase B (input)`],
    [/^HOME(\d+)$/, (m) => `QEI${m[1]} home (input)`],
    [/^INDX(\d+)$/, (m) => `QEI${m[1]} index (input)`],
    [/^QEICCMP(\d+)$/, (m) => `QEI${m[1]} counter compare (output)`],

    // External Interrupts
    [/^INT(\d+)$/, (m) => `External interrupt ${m[1]} (input)`],

    // CLC
    [/^CLC(\d+)OUT$/, (m) => `Configurable logic cell ${m[1]} (output)`],
    [/^CLCIN([A-Z])$/, (m) => `Configurable logic cell input ${m[1]} (input)`],

    // SENT (SAE J2716)
    [/^SENT(\d+)IN$/, (m) => `SENT${m[1]} data (SAE J2716, input)`],
    [/^SENT(\d+)OUT$/, (m) => `SENT${m[1]} data (SAE J2716, output)`],

    // Reference Clock
    [/^REFI$/, () => 'Reference clock (input)'],
    [/^REFO$/, () => 'Reference clock (output)'],

    // PTG
    [/^PTGTRG(\d+)$/, (m) => `PTG trigger ${m[1]} (output)`],

    // Virtual Pins
    [/^RPV(\d+)IN$/, (m) => `Virtual pin ${m[1]} (internal routing, input)`],

    // Debug / ICSP
    [/^PGC(\d+)$/, (m) => `ICSP clock / debug channel ${m[1]}`],
    [/^PGD(\d+)$/, (m) => `ICSP data / debug channel ${m[1]}`],

    // JTAG
    [/^TDI$/, () => 'JTAG test data (input)'],
    [/^TDO$/, () => 'JTAG test data (output)'],
    [/^TMS$/, () => 'JTAG test mode select'],
    [/^TCK$/, () => 'JTAG test clock'],

    // CAN (PIC18, dsPIC33E, etc.)
    [/^C(\d+)TX$/, (m) => `CAN${m[1]} transmit (output)`],
    [/^C(\d+)RX$/, (m) => `CAN${m[1]} receive (input)`],

    // USB (PIC18, PIC32)
    [/^D\+$/, () => 'USB data plus'],
    [/^D-$/, () => 'USB data minus'],
    [/^VBUS$/, () => 'USB bus voltage sense'],
    [/^VUSB$/, () => 'USB internal regulator output'],

    // CCP (PIC18)
    [/^CCP(\d+)$/, (m) => `Capture/Compare/PWM ${m[1]}`],
    [/^P(\d+)([A-D])$/, (m) => `ECCP${m[1]} output ${m[2]} (enhanced PWM)`],
];

function getDescription(name) {
    for (const [pattern, formatter] of _DESC_PATTERNS) {
        const m = name.match(pattern);
        if (m) return formatter(m);
    }
    return '';
}

// Group classification for remappable peripherals
function periphGroupFine(name) {
    // UART
    if (/^U\d+(TX|RX)$/.test(name)) return 'UART';
    if (/^U\d+(CTS|RTS|DTR|DSR)$/.test(name)) return 'UART Flow Control';

    // SPI
    if (/^SDI\d/.test(name)) return 'SPI';
    if (/^SDO\d/.test(name)) return 'SPI';
    if (/^SCK\d/.test(name)) return 'SPI';
    if (/^SS\d/.test(name)) return 'SPI';

    // CAN
    if (/^C\d+(TX|RX)$/.test(name)) return 'CAN';

    // PWM
    if (/^PWM\d+[HL]/.test(name)) return 'PWM Output';
    if (/^PWME[A-Z]/.test(name)) return 'PWM Event';
    if (/^PWMTRG/.test(name)) return 'PWM Trigger';
    if (/^PCI\d+/.test(name)) return 'PWM Fault/PCI';

    // Output Compare / SCCP / MCCP
    if (/^OCM\d/.test(name)) return 'SCCP/MCCP Output';
    if (/^OCF[A-Z]/.test(name)) return 'SCCP/MCCP Fault';

    // CCP (PIC18)
    if (/^CCP\d/.test(name)) return 'CCP';

    // Input Capture
    if (/^ICM\d/.test(name)) return 'Input Capture';

    // Timer
    if (/^T\d+CK$/.test(name)) return 'Timer';
    if (/^TCKI\d/.test(name)) return 'Timer';

    // QEI
    if (/^QE[AB]\d/.test(name)) return 'QEI';
    if (/^HOME\d/.test(name)) return 'QEI';
    if (/^INDX\d/.test(name)) return 'QEI';
    if (/^QEICCMP/.test(name)) return 'QEI';

    // Interrupts
    if (/^INT\d/.test(name)) return 'Interrupt';

    // Comparator output
    if (/^CMP\d/.test(name)) return 'Comparator';

    // CLC
    if (/^CLC\d+OUT/.test(name)) return 'CLC Output';
    if (/^CLCIN/.test(name)) return 'CLC Input';

    // SENT
    if (/^SENT/.test(name)) return 'SENT';

    // Reference clock
    if (/^REF[IO]/.test(name)) return 'Reference Clock';

    // ADC trigger
    if (/^ADCTRG/.test(name)) return 'ADC Trigger';

    // PTG
    if (/^PTGTRG/.test(name)) return 'PTG';

    // Virtual pins
    if (/^RPV\d/.test(name)) return 'Virtual Pin';

    return 'Other';
}
