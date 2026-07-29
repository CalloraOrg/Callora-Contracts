commit 66091c5f1869704e03cca07ceec10e36c03f3e9b
Author: frank0277 <uchechukwu.nwakor.245083@unn.edu.ng>
Date:   Mon Jun 29 20:12:17 2026 +0100

    feat: FIFO event archival pruning

diff --git a/contracts/settlement/src/lib.rs b/contracts/settlement/src/lib.rs
index eae9991..e4541b0 100644
--- a/contracts/settlement/src/lib.rs
+++ b/contracts/settlement/src/lib.rs
@@ -1,4 +1,5 @@
 #![no_std]
+pub mod archive;
 use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};
 
 #[contracttype]

commit 2b74e4caffd5fceebd419aa2ad35a29b4bb3ba3c
Merge: af3b023 fa54da7
Author: greatest0fallt1me <greatest0fallt1me@users.noreply.github.com>
Date:   Mon Jun 29 22:57:45 2026 +0530

    Merge PR #616

commit cd20ab962e6bb621bbf0fac651e73139de5fcc01
Merge: b4f1251 f97b627
Author: greatest0fallt1me <greatest0fallt1me@users.noreply.github.com>
Date:   Mon Jun 29 22:56:59 2026 +0530

    Merge PR #601

commit 001512f7036666a188eed3680a7e1786823b45a3
Merge: d3b56a5 9977aa7
Author: greatest0fallt1me <greatest0fallt1me@users.noreply.github.com>
Date:   Mon Jun 29 22:56:45 2026 +0530

    Merge PR #597

commit d3b56a5ef14105944d9fa775563135540863bcea
Merge: 07b3879 5b0b6ef
Author: greatest0fallt1me <greatest0fallt1me@users.noreply.github.com>
Date:   Mon Jun 29 22:56:42 2026 +0530

    Merge PR #596

commit 6f5cdc7efaf2f8f9e48d364122098bf00932161f
Merge: df9eae1 b29f412
Author: greatest0fallt1me <greatest0fallt1me@users.noreply.github.com>
Date:   Mon Jun 29 22:56:35 2026 +0530

    Merge PR #594

commit 652ddf92375490cf99272520b198b11958b11745
Merge: cc91a4b 4f6c75e
Author: greatest0fallt1me <greatest0fallt1me@users.noreply.github.com>
Date:   Mon Jun 29 22:56:22 2026 +0530

    Merge PR #590

commit cc91a4b3203c044f013bfb20445f4e3d8ca3b33e
Merge: 05d26d3 7d7022d
Author: greatest0fallt1me <greatest0fallt1me@users.noreply.github.com>
Date:   Mon Jun 29 22:56:19 2026 +0530

    Merge PR #589

commit 33a763bd6bf096aa0859328d54eb46386ad4d4e4
Merge: 9616beb f273835
Author: greatest0fallt1me <greatest0fallt1me@users.noreply.github.com>
Date:   Mon Jun 29 22:56:03 2026 +0530

    Merge PR #584

commit fa54da7642df297093e985ba848f0bd4dc929e96
Author: arisu6804 <baskarayelu@gmail.com>
Date:   Mon Jun 29 20:53:08 2026 +0530

    feat: add settlement claim batching with cursor (#554)

diff --git a/contracts/settlement/src/lib.rs b/contracts/settlement/src/lib.rs
index 8b39bfc..4b7745f 100644
--- a/contracts/settlement/src/lib.rs
+++ b/contracts/settlement/src/lib.rs
@@ -1703,6 +1703,42 @@ impl CalloraSettlement {
     pub fn migration_storage_version(env: Env) -> u32 {
         migrate::storage_version(&env)
     }
+
+    /// Batch-withdraw developer balances with a cursor for pagination.
+    ///
+    /// Processes up to `limit` (max: `MAX_BATCH_SIZE`) developers from the
+    /// provided `developers` list starting at `cursor` index.
+    ///
+    /// Each developer authorises its own withdrawal; callers that have not
+    /// called `require_auth` will cause the transaction to abort.
+    ///
+    /// Returns `(next_cursor, is_complete)`. When `is_complete` is `true` the
+    /// full list has been processed.
+    pub fn batch_withdraw_developer_balance_cursor(
+        env: Env,
+        developers: Vec<Address>,
+        amounts: Vec<i128>,
+        cursor: u32,
+        limit: u32,
+    ) -> Result<(u32, bool), SettlementError> {
+        let count = developers.len();
+        if count != amounts.len() {
+            return Err(SettlementError::AmountNotPositive); // mismatched inputs
+        }
+        let safe_limit = limit.min(MAX_BATCH_SIZE);
+        let start = cursor as usize;
+        let end = (start + safe_limit as usize).min(count as usize);
+
+        for i in start..end {
+            let developer = developers.get(i as u32).ok_or(SettlementError::InsufficientDeveloperBalance)?;
+            let amount = amounts.get(i as u32).ok_or(SettlementError::AmountNotPositive)?;
+            Self::withdraw_developer_balance(env.clone(), developer, amount, None)?;
+        }
+
+        let next_cursor = end as u32;
+        let is_complete = next_cursor >= count;
+        Ok((next_cursor, is_complete))
+    }
 }
 
 mod events;

commit 9977aa754276bca2859f130622dc6f4c24497ede
Author: ola196 <oladayoshola25@gmail.com>
Date:   Mon Jun 29 11:07:13 2026 +0000

    feat: cross-contract cumulative conservation invariants
    
    - vault: add TotalDeducted storage key + get_total_deducted() view
    - settlement: add TotalReceived storage key + get_total_received() view
    - test_cross_invariant: 64-seed x 48-step end-to-end conservation check
    - docs: update vault.json and settlement.json interface summaries
    
    Closes #539

diff --git a/contracts/settlement/src/lib.rs b/contracts/settlement/src/lib.rs
index 86ad38d..a60fee6 100644
--- a/contracts/settlement/src/lib.rs
+++ b/contracts/settlement/src/lib.rs
@@ -26,6 +26,8 @@ pub enum StorageKey {
     DailyWithdrawCap(Address),
     WithdrawalToday(Address),
     ContractVersion,
+    /// Cumulative total of all funds received via `receive_payment` / `batch_receive_payment`.
+    TotalReceived,
 }
 
 /// Developer balance record in settlement contract
@@ -287,6 +289,15 @@ impl CalloraSettlement {
                 },
             );
         }
+        // Increment cumulative received total regardless of routing (pool or developer).
+        let inst = env.storage().instance();
+        let prev: i128 = inst.get(&StorageKey::TotalReceived).unwrap_or(0i128);
+        inst.set(
+            &StorageKey::TotalReceived,
+            &prev
+                .checked_add(amount)
+                .unwrap_or_else(|| env.panic_with_error(SettlementError::PoolOverflow)),
+        );
     }
 
     /// Atomically credit multiple developer balances in a single call.
@@ -360,6 +371,18 @@ impl CalloraSettlement {
                 },
             );
         }
+        // Increment cumulative received total by the batch sum.
+        let batch_total: i128 = items.iter().map(|(_, a)| a).fold(0i128, |acc, a| {
+            acc.checked_add(a)
+                .unwrap_or_else(|| env.panic_with_error(SettlementError::PoolOverflow))
+        });
+        let prev: i128 = inst.get(&StorageKey::TotalReceived).unwrap_or(0i128);
+        inst.set(
+            &StorageKey::TotalReceived,
+            &prev
+                .checked_add(batch_total)
+                .unwrap_or_else(|| env.panic_with_error(SettlementError::PoolOverflow)),
+        );
     }
 
     /// Get current admin address
@@ -386,6 +409,23 @@ impl CalloraSettlement {
             .unwrap_or_else(|| env.panic_with_error(SettlementError::NotInitialized))
     }
 
+    /// Return the cumulative total of all funds received via `receive_payment`
+    /// and `batch_receive_payment`.
+    ///
+    /// Returns `0` before any payments are received. Uses overflow-safe arithmetic.
+    ///
+    /// # Conservation invariant
+    /// `get_total_received() == Σ developer_balance[i] + global_pool.total_balance + Σ withdrawn`
+    pub fn get_total_received(env: Env) -> i128 {
+        if !env.storage().instance().has(&StorageKey::Admin) {
+            env.panic_with_error(SettlementError::NotInitialized);
+        }
+        env.storage()
+            .instance()
+            .get(&StorageKey::TotalReceived)
+            .unwrap_or(0i128)
+    }
+
     /// Get developer balance
     ///
     /// Performs a direct O(1) persistent storage lookup for the specified developer's balance.

commit e9f9e84ee1b5a7c75f4b9397820f3c5ce797677a
Author: root <root@vmi3321457.contaboserver.net>
Date:   Mon Jun 29 10:50:19 2026 +0200

    fix: remove unused alloc import (global allocator error) and update u16::MAX to u32::MAX in vault tests

diff --git a/contracts/settlement/src/lib.rs b/contracts/settlement/src/lib.rs
index 2b35f36..8c431a6 100644
--- a/contracts/settlement/src/lib.rs
+++ b/contracts/settlement/src/lib.rs
@@ -9,9 +9,6 @@ pub const MAX_BATCH_SIZE: u32 = 50;
 
 /// Maximum number of developer balances returned per page in paginated queries.
 pub const MAX_DEVELOPER_BALANCES_PAGE_SIZE: u32 = 100;
-extern crate alloc;
-
-use alloc::string::String;
 
 mod admin;
 mod errors;

commit 3205cc4dea8a2324a5d4939aa09f43e58673c43b
Author: root <root@vmi3321457.contaboserver.net>
Date:   Mon Jun 29 10:49:43 2026 +0200

    fix: remove unused alloc import (global allocator error) and update u16::MAX to u32::MAX in vault tests

diff --git a/contracts/settlement/src/lib.rs b/contracts/settlement/src/lib.rs
index e90cf4c..9492a94 100644
--- a/contracts/settlement/src/lib.rs
+++ b/contracts/settlement/src/lib.rs
@@ -9,9 +9,6 @@ pub const MAX_BATCH_SIZE: u32 = 50;
 
 /// Maximum number of developer balances returned per page in paginated queries.
 pub const MAX_DEVELOPER_BALANCES_PAGE_SIZE: u32 = 100;
-extern crate alloc;
-
-use alloc::string::String;
 
 mod admin;
 mod errors;

commit d07f5f97d1f3a50eafff5f9350953825df3d4b00
Author: root <root@vmi3321457.contaboserver.net>
Date:   Mon Jun 29 10:49:31 2026 +0200

    fix: remove unused alloc import (global allocator error) and update u16::MAX to u32::MAX in vault tests

diff --git a/contracts/settlement/src/lib.rs b/contracts/settlement/src/lib.rs
index 25ecac2..d3fd935 100644
--- a/contracts/settlement/src/lib.rs
+++ b/contracts/settlement/src/lib.rs
@@ -9,9 +9,6 @@ pub const MAX_BATCH_SIZE: u32 = 50;
 
 /// Maximum number of developer balances returned per page in paginated queries.
 pub const MAX_DEVELOPER_BALANCES_PAGE_SIZE: u32 = 100;
-extern crate alloc;
-
-use alloc::string::String;
 
 mod admin;
 mod errors;

commit 74ed74e8216b3f2e108230b1fabbf7378976880d
Author: root <root@vmi3321457.contaboserver.net>
Date:   Mon Jun 29 10:49:01 2026 +0200

    fix: remove unused alloc import causing global allocator build error

diff --git a/contracts/settlement/src/lib.rs b/contracts/settlement/src/lib.rs
index e731935..192f809 100644
--- a/contracts/settlement/src/lib.rs
+++ b/contracts/settlement/src/lib.rs
@@ -9,9 +9,7 @@ pub const MAX_BATCH_SIZE: u32 = 50;
 
 /// Maximum number of developer balances returned per page in paginated queries.
 pub const MAX_DEVELOPER_BALANCES_PAGE_SIZE: u32 = 100;
-extern crate alloc;
 
-use alloc::string::String;
 
 mod admin;
 mod errors;
