import hashlib
from pathlib import Path
import tempfile
import unittest

from release import validate


class ReleaseValidation(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        names = [
            'kestrel_0.2.0_amd64.deb', 'Kestrel-0.2.0-x86_64.AppImage',
            'Kestrel-0.2.0-aarch64.dmg', 'Kestrel-0.2.0-x86_64.dmg', 'Kestrel-0.2.0-universal.dmg',
            'Kestrel-0.2.0-x86_64-windows.zip', 'Kestrel-0.2.0-x86_64-setup.exe',
        ]
        for name in names:
            payload = f'Package {name}'.encode()
            (self.directory / name).write_bytes(payload)
            digest = hashlib.sha256(payload).hexdigest()
            (self.directory / f'{name}.sha256').write_text(f'{digest}  {name}\n')

    def test_all_platforms_and_checksums_are_required(self):
        self.assertEqual(len(validate(self.directory, '0.2.0')), 14)
        (self.directory / 'Kestrel-0.2.0-aarch64.dmg').unlink()
        with self.assertRaisesRegex(ValueError, 'missing='):
            validate(self.directory, '0.2.0')

    def test_changed_packages_cannot_be_published(self):
        (self.directory / 'Kestrel-0.2.0-x86_64-setup.exe').write_bytes(b'changed')
        with self.assertRaisesRegex(ValueError, 'SHA256 mismatch'):
            validate(self.directory, '0.2.0')

    def test_old_version_files_cannot_be_mixed_into_a_new_release(self):
        (self.directory / 'Kestrel-0.1.0-aarch64.dmg').write_bytes(b'old')
        with self.assertRaisesRegex(ValueError, 'extra='):
            validate(self.directory, '0.2.0')


if __name__ == '__main__':
    unittest.main()
