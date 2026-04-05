const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { execFileSync } = require('node:child_process');
const path = require('node:path');

const repoRoot = path.join(__dirname, '..');
const upperForbidden = String.fromCharCode(67, 108, 97, 117, 100, 101);
const lowerForbidden = String.fromCharCode(99, 108, 97, 117, 100, 101);

test('tracked repository files do not contain retired provider branding text', () => {
    const tracked = execFileSync('git', ['ls-files', '-z'], {
        cwd: repoRoot,
        encoding: 'utf8',
        stdio: ['ignore', 'pipe', 'pipe'],
    })
        .split('\0')
        .filter(Boolean)
        .filter((file) => file !== 'tests/branding-hygiene.test.js');

    const offenders = [];
    for (const relativePath of tracked) {
        if (relativePath.includes(upperForbidden) || relativePath.includes(lowerForbidden)) {
            offenders.push(relativePath);
            continue;
        }

        const absolutePath = path.join(repoRoot, relativePath);
        if (!fs.existsSync(absolutePath)) {
            continue;
        }

        const content = fs.readFileSync(absolutePath, 'utf8');
        if (content.includes(upperForbidden) || content.includes(lowerForbidden)) {
            offenders.push(relativePath);
        }
    }

    assert.deepEqual(offenders, []);
});
