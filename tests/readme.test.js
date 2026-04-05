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
        /pickle is an independent project and is not affiliated with, endorsed by, sponsored by, or approved by Microchip Technology Inc\./
    );
    assert.match(
        readme,
        /pickle is built to use publicly available technical information together with user-supplied or separately downloaded device data\./
    );
    assert.match(
        readme,
        /The repository and application distribution do not include or redistribute Microchip-owned datasheets, device packs, images, or other source materials\./
    );
    assert.match(
        readme,
        /Microchip, dsPIC33, PIC24, and related product names, trademarks, logos, and brand names are the property of Microchip Technology Inc\./
    );
    assert.match(
        readme,
        /All rights in that intellectual property remain with Microchip Technology Inc\./
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

test('README keeps the legal disclaimer at the bottom after the license section', () => {
    const readme = readReadme();

    const licenseIndex = readme.indexOf('## License');
    const disclaimerIndex = readme.indexOf('## Legal Disclaimer');

    assert.notEqual(licenseIndex, -1);
    assert.notEqual(disclaimerIndex, -1);
    assert.ok(disclaimerIndex > licenseIndex);
    assert.equal(
        readme.trimEnd().endsWith('<!-- mandatory-readme-legal-end -->'),
        true
    );
});
