import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import '../support/platform_asset_contract.dart';

void main() {
  final pubspec = File('pubspec.yaml').readAsStringSync();
  final assets = Directory('assets')
      .listSync(recursive: true, followLinks: false)
      .whereType<File>()
      .map((file) => file.path.replaceAll('\\', '/'))
      .where((path) => !path.split('/').any((part) => part.startsWith('.')))
      .toList();
  const desktopEntry = '''    - path: assets/illustrations/desktop/
      platforms:
        - linux
        - macos
        - web
        - windows''';

  test('pubspec exposes runtime assets on their intended platforms', () {
    expect(platformAssetContractErrors(pubspec, assets), isEmpty);
  });

  test('an unrestricted desktop declaration fails the contract', () {
    expect(pubspec, contains(desktopEntry));
    final changed = pubspec.replaceFirst(
      desktopEntry,
      '    - assets/illustrations/desktop/',
    );
    expect(
      platformAssetContractErrors(changed, assets),
      contains(contains('unexpected [android, ios]')),
    );
  });

  test('a shared profile directory restricted to desktop fails', () {
    const profileEntry = '    - assets/profile_pictures/';
    expect(pubspec, contains(profileEntry));
    final changed = pubspec.replaceFirst(
      profileEntry,
      '''    - path: assets/profile_pictures/
      platforms:
        - macos''',
    );
    expect(
      platformAssetContractErrors(changed, assets),
      contains(allOf(contains('profile_picture_01.png'), contains('android'))),
    );
  });

  test('an undeclared nested directory fails even with a declared parent', () {
    const nested = 'assets/illustrations/new_scene/hero.webp';
    expect(
      platformAssetContractErrors(pubspec, [...assets, nested]),
      contains(startsWith('$nested: missing')),
    );
  });

  test('an unrestricted duplicate cannot override desktop exclusion', () {
    final changed = pubspec.replaceFirst(
      '  assets:',
      '  assets:\n    - assets/icons/desktop/network_zec.png',
    );
    expect(
      platformAssetContractErrors(changed, assets),
      contains(allOf(contains('network_zec.png'), contains('android, ios'))),
    );
  });

  test('declared assets retain resolution variants and fonts', () {
    expect(
      platformAssetContractErrors(
        '''
flutter:
  assets:
    - assets/icons/
    - assets/images/hero.webp
  fonts:
    - family: Test
      fonts:
        - asset: assets/fonts/test.ttf
''',
        [
          'assets/icons/book.svg',
          'assets/icons/2.0x/book.svg',
          'assets/images/hero.webp',
          'assets/images/3.0x/hero.webp',
          'assets/images/2.x/hero.webp',
          'assets/fonts/test.ttf',
        ],
      ),
      isEmpty,
    );
  });
}
