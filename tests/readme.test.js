const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const readmePath = path.join(__dirname, '..', 'README.md');

function readReadme() {
    return fs.readFileSync(readmePath, 'utf8');
}

test('README keeps the required Microchip legal disclaimer', () => {
    const readme = readReadme();

    assert.match(readme, /<!-- mandatory-readme-legal-start -->/);
    assert.match(readme, /<!-- mandatory-readme-legal-end -->/);
    assert.match(
        readme,
        /pickle is an independent project and has no affiliation with, endorsement from, sponsorship from, or approval by Microchip Technology Inc\./
    );
    assert.match(
        readme,
        /All Microchip intellectual property referenced by this project, including Microchip, dsPIC33, PIC24, and related product names, trademarks, and brand names, belongs to Microchip Technology Inc\./
    );
});

test('README advertises the GPLv3 license and no longer claims MIT', () => {
    const readme = readReadme();

    assert.match(
        readme,
        /## License\s+GNU General Public License v3\.0 \(GPLv3\)\. See \[LICENSE\]\(LICENSE\)\./
    );
    assert.doesNotMatch(readme, /^MIT$/m);
});
