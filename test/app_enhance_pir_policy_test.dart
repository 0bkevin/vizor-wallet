import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zcash_wallet/app.dart';
import 'package:zcash_wallet/src/app_bootstrap.dart';
import 'package:zcash_wallet/src/core/config/rpc_endpoint_config.dart';
import 'package:zcash_wallet/src/providers/account_provider.dart';
import 'package:zcash_wallet/src/providers/enhance_pir_provider.dart';

AppBootstrapState _ready({required bool enabled}) => AppBootstrapState(
  initialLocation: '/home',
  initialAccountState: const AccountState(),
  initialSyncSnapshot: AppSyncSnapshot.empty,
  network: 'main',
  rpcEndpointConfig: defaultRpcEndpointConfig('main'),
  themeMode: ThemeMode.system,
  privacyModeEnabled: false,
  isPasswordConfigured: true,
  isUnlocked: true,
  passwordRotationRecoveryFailed: false,
  enhancePirEnabled: enabled,
);

void main() {
  late List<String> applied;

  Future<void> apply(AppBootstrapState bootstrap) => applyEnhancePirPolicy(
    bootstrap,
    setRustEnabled: (enabled) => applied.add('rust:$enabled'),
    setNativePrivateRecovery: (enabled) async => applied.add('native:$enabled'),
  );

  setUp(() => applied = []);

  for (final kind in AppBootstrapFailureKind.values) {
    test(
      'a blocked bootstrap ($kind) leaves native private mode alone',
      () async {
        // The blocked state carries a default `false`, not the saved preference.
        final blocked = AppBootstrapState.blocked(
          failureKind: kind,
          failureMessage: 'blocked',
        );
        expect(blocked.enhancePirEnabled, isFalse);

        await apply(blocked);

        expect(applied, isEmpty);
      },
    );
  }

  test(
    'a ready bootstrap applies the saved preference to both sides',
    () async {
      await apply(_ready(enabled: true));
      // Masquerade builds never enable the production service.
      final expected = isEnhancePirAvailableForNetwork('main');
      expect(applied, ['rust:$expected', 'native:$expected']);
    },
  );

  test('a ready bootstrap applies a disabled preference', () async {
    await apply(_ready(enabled: false));
    expect(applied, ['rust:false', 'native:false']);
  });
}
