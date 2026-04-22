#![allow(non_snake_case)]

use crate::gui::AppState;
use crate::model::*;
use crate::store::observer::{EventObserving, Interest};
use qmetaobject::{prelude::*, QMetaType};
use qttypes::{QVariantList, QVariantMap};
use whisperfish_store::schema;

/// QML model for displaying receipts for a specific message
#[derive(Default, QObject)]
pub struct Receipts {
    base: qt_base_class!(trait QObject),

    app: qt_property!(QPointer<AppState>; WRITE set_app),
    message_id: qt_property!(i32; WRITE set_message_id ALIAS messageId),

    delivery_receipts: qt_property!(QVariant; READ delivery_receipts NOTIFY receipts_changed ALIAS delivered),
    read_receipts: qt_property!(QVariant; READ read_receipts NOTIFY receipts_changed ALIAS read),
    viewed_receipts: qt_property!(QVariant; READ viewed_receipts NOTIFY receipts_changed ALIAS viewed),

    receipts_changed: qt_signal!(),
}

impl EventObserving for Receipts {
    type Context = ModelContext<Self>;

    fn observe(&mut self, _ctx: Self::Context, event: crate::store::observer::Event) {
        if event.for_table(schema::receipts::table) && !self.app.is_null() && self.message_id > 0 {
            self.receipts_changed();
        }
    }

    fn interests(&self) -> Vec<Interest> {
        let message_id = self.message_id;
        if message_id >= 0 {
            return vec![Interest::whole_table_with_relation(
                schema::receipts::table,
                schema::messages::table,
                message_id,
            )];
        }
        Vec::new()
    }
}

impl Receipts {
    fn set_app(&mut self, app: QPointer<AppState>) {
        self.app = app;
        if self.message_id > 0 {
            self.receipts_changed();
        }
    }

    fn set_message_id(&mut self, id: i32) {
        self.message_id = id;
        if !self.app.is_null() {
            self.receipts_changed();
        }
    }

    fn delivery_receipts(&self) -> QVariant {
        self.get_receipts_by_type(ReceiptType::Delivered)
    }

    fn read_receipts(&self) -> QVariant {
        self.get_receipts_by_type(ReceiptType::Read)
    }

    fn viewed_receipts(&self) -> QVariant {
        self.get_receipts_by_type(ReceiptType::Viewed)
    }

    fn get_receipts_by_type(&self, receipt_type: ReceiptType) -> QVariant {
        let message_id = self.message_id;
        if message_id >= 0 {
            if let Some(app) = self.app.as_pinned() {
                if let Some(storage) = app.borrow().storage.borrow().clone() {
                    let mut variant_list = QVariantList::default();

                    let receipts = storage.fetch_message_receipts(message_id);
                    for (receipt, recipient) in receipts.iter() {
                        let timestamp = match receipt_type {
                            ReceiptType::Delivered => receipt.delivered,
                            ReceiptType::Read => receipt.read,
                            ReceiptType::Viewed => receipt.viewed,
                        };

                        let Some(timestamp) = timestamp else {
                            continue;
                        };

                        let mut item = QVariantMap::default();
                        item.insert(
                            "recipient".into(),
                            recipient.name().to_string().to_qvariant(),
                        );
                        item.insert(
                            "timestamp".into(),
                            qdatetime_from_naive(timestamp).to_qvariant(),
                        );
                        variant_list.push(item.to_qvariant());
                    }

                    return variant_list.to_qvariant();
                }
            }
        }
        QVariantList::default().to_qvariant()
    }
}

#[derive(Clone, Copy)]
enum ReceiptType {
    Delivered,
    Read,
    Viewed,
}
