@Tags(['mobile'])
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zcash_wallet/src/app_bootstrap.dart';
import 'package:zcash_wallet/src/core/config/rpc_endpoint_config.dart';
import 'package:zcash_wallet/src/core/layout/mobile/app_mobile_sheet.dart';
import 'package:zcash_wallet/src/core/profile_pictures.dart';
import 'package:zcash_wallet/src/core/theme/app_theme.dart';
import 'package:zcash_wallet/src/features/receive/screens/mobile/mobile_receive_screen.dart';
import 'package:zcash_wallet/src/features/receive/widgets/request/request_qr_surface.dart';
import 'package:zcash_wallet/src/providers/account_provider.dart';
import 'package:zcash_wallet/src/providers/receive_address_provider.dart';
import 'package:zcash_wallet/src/providers/sync_provider.dart';
import 'package:zcash_wallet/src/providers/zec_price_change_provider.dart';

import '../../fakes/fake_sync_notifier.dart';
import '../../figma_compare/figma_compare_font_loader.dart';

const _address = 'u1tvg2412a23kshieldedaddressk64123hhq6d';
const _memo = 'QA_keyboard_check';
const _requestUri = 'zcash:$_address?amount=1&memo=UUFfa2V5Ym9hcmRfY2hlY2s';

const _account = AccountState(
  accounts: [
    AccountInfo(
      uuid: 'account-1',
      name: 'Test account',
      order: 0,
      profilePictureId: kDefaultProfilePictureId,
    ),
  ],
  activeAccountUuid: 'account-1',
  activeAddress: _address,
);

AppBootstrapState _bootstrap() => AppBootstrapState(
  initialLocation: '/receive',
  initialAccountState: _account,
  initialSyncSnapshot: AppSyncSnapshot.empty,
  network: 'main',
  rpcEndpointConfig: defaultRpcEndpointConfig('main'),
  themeMode: ThemeMode.dark,
  privacyModeEnabled: false,
  isPasswordConfigured: true,
  isUnlocked: true,
  passwordRotationRecoveryFailed: false,
);

class _ReceiveAddresses implements ReceiveAddressService {
  @override
  Future<String> loadShieldedAddress({
    required String accountUuid,
    String? currentShieldedAddress,
  }) async => currentShieldedAddress ?? _address;

  @override
  Future<String> loadTransparentReceiveAddress({
    required String accountUuid,
  }) async => 't1aWwWwqk3jYGkZc7nLGuTvuM8hDywMZCo';

  @override
  Future<String> renewShieldedAddress({required String accountUuid}) async =>
      _address;

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

Widget _app({double textScale = 1}) => ProviderScope(
  overrides: [
    appBootstrapProvider.overrideWithValue(_bootstrap()),
    syncProvider.overrideWith(() => FakeSyncNotifier(SyncState())),
    receiveAddressServiceProvider.overrideWithValue(_ReceiveAddresses()),
    zecLiveUsdUnitPriceProvider.overrideWithValue(70),
  ],
  child: MaterialApp(
    builder: (context, child) => MediaQuery(
      data: MediaQuery.of(
        context,
      ).copyWith(textScaler: TextScaler.linear(textScale)),
      child: AppTheme(
        data: AppThemeData.dark,
        child: GestureDetector(
          behavior: HitTestBehavior.translucent,
          onTap: () => FocusManager.instance.primaryFocus?.unfocus(),
          child: child!,
        ),
      ),
    ),
    home: const MobileReceiveScreen(),
  ),
);

Future<void> _pumpPhone(
  WidgetTester tester, {
  Size size = const Size(384, 823.4667),
  double textScale = 1,
}) async {
  // Samsung SM-S908U: 384×823dp at font scale 1. The native Gboard report
  // measured 332.09dp of IME inset; its view padding is separate from the
  // top status-bar padding used by showModalBottomSheet's SafeArea.
  tester.view.physicalSize = size;
  tester.view.devicePixelRatio = 1;
  tester.view.padding = const FakeViewPadding(top: 26.6667);
  tester.view.viewPadding = const FakeViewPadding(
    top: 26.6667,
    bottom: 14.9333,
  );
  addTearDown(tester.view.reset);

  await tester.pumpWidget(_app(textScale: textScale));
  await _settle(tester);
}

Future<void> _settle(WidgetTester tester) async {
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 300));
  await tester.pump(const Duration(milliseconds: 300));
}

Future<void> _openRequestWithMemo(
  WidgetTester tester, {
  double keyboardInset = 332.0889,
}) async {
  final requestEntry = find.byKey(const ValueKey('mobile_receive_request'));
  await tester.scrollUntilVisible(
    requestEntry,
    120,
    scrollable: find
        .descendant(
          of: find.byType(MobileReceiveScreen),
          matching: find.byType(Scrollable),
        )
        .first,
  );
  await tester.tap(requestEntry);
  await _settle(tester);
  await tester.enterText(
    find.byKey(const ValueKey('request_amount_input')),
    '1',
  );
  await tester.pump();

  // The amount keyboard is dismissed before opening the memo editor.
  FocusManager.instance.primaryFocus?.unfocus();
  tester.view.viewInsets = const FakeViewPadding(bottom: 0);
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 300));

  await tester.tap(find.byKey(const ValueKey('request_message_row')));
  await tester.pump();
  await tester.enterText(
    find.byKey(const ValueKey('request_message_field')),
    _memo,
  );
  await tester.pump();

  tester.view.viewInsets = FakeViewPadding(bottom: keyboardInset);
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 400));
}

void main() {
  setUpAll(loadFigmaCompareFonts);

  for (final (keyboard, inset) in <(String, double)>[
    ('Gboard', 332.0889),
    ('Samsung', 358.4),
    ('closed', 0),
  ]) {
    for (final (position, fraction) in <(String, double)>[
      ('upper', 0.1),
      ('center', 0.5),
      ('lower', 0.86),
    ]) {
      testWidgets(
        'creates the request from the $position of the button with keyboard $keyboard',
        (tester) async {
          await _pumpPhone(tester);
          await _openRequestWithMemo(tester, keyboardInset: inset);
          if (inset == 0) {
            FocusManager.instance.primaryFocus?.unfocus();
            await tester.pump();
          } else {
            expect(tester.testTextInput.isVisible, isTrue);
          }

          // The request body must fit its keyboard-limited card. On latest main,
          // the unbounded compose Column overflows at this measured inset and
          // clips the lower part of the primary action.
          expect(tester.takeException(), isNull);
          final button = find.byKey(const ValueKey('request_create_button'));
          final buttonRect = tester.getRect(button);
          expect(buttonRect.height, AppButtonSizing.largeHeight);

          final cardSurface = find.descendant(
            of: find.byType(MobileModalCard),
            matching: find.byType(Material),
          );
          expect(cardSurface, findsOneWidget);
          final cardRect = tester.getRect(cardSurface);
          expect(cardRect.bottom, lessThanOrEqualTo(823.4667 - inset));
          expect(buttonRect.top, greaterThanOrEqualTo(cardRect.top));
          expect(buttonRect.bottom, lessThanOrEqualTo(cardRect.bottom));
          expect(
            cardRect.bottom - buttonRect.bottom,
            inset > 0 ? AppSpacing.sm : AppSpacing.base,
          );

          await tester.tapAt(
            Offset(
              buttonRect.center.dx,
              buttonRect.top + buttonRect.height * fraction,
            ),
          );
          await tester.idle();
          expect(tester.testTextInput.isVisible, isFalse);
          // The native IME hides when the button removes focus; reflect that
          // platform inset change before the next layout frame.
          tester.view.viewInsets = const FakeViewPadding(bottom: 0);
          await tester.pump();
          await tester.pump(const Duration(milliseconds: 400));

          expect(find.byType(RequestQrSurface), findsOneWidget);
          expect(
            tester.widget<RequestQrSurface>(find.byType(RequestQrSurface)).data,
            _requestUri,
          );
          expect(tester.takeException(), isNull);
        },
        variant: TargetPlatformVariant({
          TargetPlatform.android,
          if (inset == 0) TargetPlatform.iOS,
        }),
      );
    }
  }

  testWidgets('restores the request footer spacing when the keyboard closes', (
    tester,
  ) async {
    await _pumpPhone(tester);
    await _openRequestWithMemo(tester, keyboardInset: 0);
    final button = find.byKey(const ValueKey('request_create_button'));
    final card = find.byType(MobileModalCard);
    final cardSurface = find.descendant(
      of: card,
      matching: find.byType(Material),
    );
    final fields = find.descendant(
      of: card,
      matching: find.byType(SingleChildScrollView),
    );

    for (final inset in [0.0, 332.0889, 0.0]) {
      tester.view.viewInsets = FakeViewPadding(bottom: inset);
      await _settle(tester);
      final buttonRect = tester.getRect(button);
      expect(buttonRect.height, AppButtonSizing.largeHeight);
      expect(
        buttonRect.top - tester.getRect(fields).bottom,
        inset > 0 ? AppSpacing.sm : AppSpacing.md,
      );
      expect(
        tester.getRect(cardSurface).bottom - buttonRect.bottom,
        inset > 0 ? AppSpacing.sm : AppSpacing.base,
      );
      expect(tester.takeException(), isNull);
    }

    await tester.tap(button);
    await _settle(tester);
    expect(
      tester.widget<RequestQrSurface>(find.byType(RequestQrSurface)).data,
      _requestUri,
    );
  }, variant: TargetPlatformVariant({TargetPlatform.android}));

  testWidgets(
    'keeps the action reachable with enlarged text on a narrow phone',
    (tester) async {
      await _pumpPhone(tester, size: const Size(320, 700), textScale: 1.5);
      await _openRequestWithMemo(tester, keyboardInset: 256);
      expect(tester.takeException(), isNull);
      final button = find.byKey(const ValueKey('request_create_button'));
      expect(tester.getRect(button).bottom, lessThanOrEqualTo(700 - 256));
      await tester.tap(button);
      await tester.idle();
      expect(tester.testTextInput.isVisible, isFalse);
      tester.view.viewInsets = const FakeViewPadding();
      await _settle(tester);
      expect(
        tester.widget<RequestQrSurface>(find.byType(RequestQrSurface)).data,
        _requestUri,
      );
      expect(tester.takeException(), isNull);
    },
    variant: TargetPlatformVariant({TargetPlatform.android}),
  );
}
