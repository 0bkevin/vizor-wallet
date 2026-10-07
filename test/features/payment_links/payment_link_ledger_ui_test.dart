import 'package:flutter_test/flutter_test.dart';
import 'package:zcash_wallet/src/features/payment_links/services/payment_link_batch_limits.dart';
import 'package:zcash_wallet/src/providers/account_models.dart';

void main() {
  test('Ledger review budget leaves other signers unchanged', () {
    expect(paymentLinkBatchMaxCount(HardwareSignerKind.ledger), 4);
    expect(paymentLinkBatchMaxCount(HardwareSignerKind.keystone), 30);
    expect(paymentLinkBatchMaxCount(null), 50);
  });
}
