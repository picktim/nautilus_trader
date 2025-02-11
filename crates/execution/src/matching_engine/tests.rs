// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2025 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
//  You may not use this file except in compliance with the License.
//  You may obtain a copy of the License at https://www.gnu.org/licenses/lgpl-3.0.en.html
//
//  Unless required by applicable law or agreed to in writing, software
//  distributed under the License is distributed on an "AS IS" BASIS,
//  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//  See the License for the specific language governing permissions and
//  limitations under the License.
// -------------------------------------------------------------------------------------------------

use std::{cell::RefCell, rc::Rc, sync::LazyLock};

use chrono::{DateTime, TimeZone, Utc};
use nautilus_common::{
    cache::Cache,
    msgbus::{
        handler::ShareableMessageHandler,
        stubs::{get_message_saving_handler, get_saved_messages},
        MessageBus,
    },
};
use nautilus_core::{AtomicTime, UnixNanos, UUID4};
use nautilus_model::{
    data::{stubs::OrderBookDeltaTestBuilder, BookOrder, TradeTick},
    enums::{
        AccountType, AggressorSide, BookAction, BookType, ContingencyType, LiquiditySide, OmsType,
        OrderSide, OrderType, TimeInForce,
    },
    events::{
        order::rejected::OrderRejectedBuilder, OrderEventAny, OrderEventType, OrderFilled,
        OrderRejected,
    },
    identifiers::{
        stubs::account_id, AccountId, ClientId, ClientOrderId, InstrumentId, PositionId,
        StrategyId, TradeId, TraderId, VenueOrderId,
    },
    instruments::{
        stubs::{crypto_perpetual_ethusdt, equity_aapl, futures_contract_es},
        CryptoPerpetual, Equity, InstrumentAny,
    },
    orders::{stubs::TestOrderStubs, OrderAny, OrderTestBuilder},
    types::{Price, Quantity},
};
use rstest::{fixture, rstest};
use ustr::Ustr;

use crate::{
    matching_engine::{config::OrderMatchingEngineConfig, engine::OrderMatchingEngine},
    messages::{BatchCancelOrders, CancelAllOrders, CancelOrder, ModifyOrder},
    models::{fee::FeeModelAny, fill::FillModel},
};

static ATOMIC_TIME: LazyLock<AtomicTime> =
    LazyLock::new(|| AtomicTime::new(true, UnixNanos::default()));

#[fixture]
fn msgbus() -> MessageBus {
    MessageBus::default()
}

#[fixture]
pub fn time() -> AtomicTime {
    AtomicTime::new(false, UnixNanos::default())
}

#[fixture]
fn order_event_handler() -> ShareableMessageHandler {
    get_message_saving_handler::<OrderEventAny>(Some(Ustr::from("ExecEngine.process")))
}

#[fixture]
pub fn instrument_eth_usdt(crypto_perpetual_ethusdt: CryptoPerpetual) -> InstrumentAny {
    InstrumentAny::CryptoPerpetual(crypto_perpetual_ethusdt)
}

// Market buy order with corresponding fill
#[fixture]
pub fn market_order_buy(instrument_eth_usdt: InstrumentAny) -> OrderAny {
    OrderTestBuilder::new(OrderType::Market)
        .instrument_id(instrument_eth_usdt.id())
        .side(OrderSide::Buy)
        .quantity(Quantity::from("1.000"))
        .client_order_id(ClientOrderId::from("O-19700101-000000-001-001-1"))
        .build()
}

#[fixture]
pub fn market_order_fill(
    instrument_eth_usdt: InstrumentAny,
    account_id: AccountId,
    market_order_buy: OrderAny,
) -> OrderFilled {
    OrderFilled::new(
        market_order_buy.trader_id(),
        market_order_buy.strategy_id(),
        market_order_buy.instrument_id(),
        market_order_buy.client_order_id(),
        VenueOrderId::new("BINANCE-1"),
        account_id,
        TradeId::new("1"),
        market_order_buy.order_side(),
        market_order_buy.order_type(),
        Quantity::from("1"),
        Price::from("1000.000"),
        instrument_eth_usdt.quote_currency(),
        LiquiditySide::Taker,
        UUID4::new(),
        UnixNanos::default(),
        UnixNanos::default(),
        false,
        Some(PositionId::new("P-1")),
        None,
    )
}

// Market sell order
#[fixture]
pub fn market_order_sell(instrument_eth_usdt: InstrumentAny) -> OrderAny {
    OrderTestBuilder::new(OrderType::Market)
        .instrument_id(instrument_eth_usdt.id())
        .side(OrderSide::Sell)
        .quantity(Quantity::from("1.000"))
        .client_order_id(ClientOrderId::from("O-19700101-000000-001-001-2"))
        .build()
}

// For valid ES futures contract currently active
#[fixture]
fn instrument_es() -> InstrumentAny {
    let activation = UnixNanos::from(
        Utc.with_ymd_and_hms(2022, 4, 8, 0, 0, 0)
            .unwrap()
            .timestamp_nanos_opt()
            .unwrap() as u64,
    );
    let expiration = UnixNanos::from(
        Utc.with_ymd_and_hms(2100, 7, 8, 0, 0, 0)
            .unwrap()
            .timestamp_nanos_opt()
            .unwrap() as u64,
    );
    InstrumentAny::FuturesContract(futures_contract_es(Some(activation), Some(expiration)))
}

#[fixture]
fn engine_config() -> OrderMatchingEngineConfig {
    OrderMatchingEngineConfig {
        bar_execution: false,
        reject_stop_orders: false,
        support_gtd_orders: false,
        support_contingent_orders: true,
        use_position_ids: false,
        use_random_ids: false,
        use_reduce_only: true,
    }
}
// -- HELPERS ---------------------------------------------------------------------------

fn get_order_matching_engine(
    instrument: InstrumentAny,
    msgbus: Rc<RefCell<MessageBus>>,
    cache: Option<Rc<RefCell<Cache>>>,
    account_type: Option<AccountType>,
    config: Option<OrderMatchingEngineConfig>,
) -> OrderMatchingEngine {
    let cache = cache.unwrap_or(Rc::new(RefCell::new(Cache::default())));
    let config = config.unwrap_or_default();
    OrderMatchingEngine::new(
        instrument,
        1,
        FillModel::default(),
        FeeModelAny::default(),
        BookType::L1_MBP,
        OmsType::Netting,
        account_type.unwrap_or(AccountType::Cash),
        &ATOMIC_TIME,
        msgbus,
        cache,
        config,
    )
}

fn get_order_matching_engine_l2(
    instrument: InstrumentAny,
    msgbus: Rc<RefCell<MessageBus>>,
    cache: Option<Rc<RefCell<Cache>>>,
    account_type: Option<AccountType>,
    config: Option<OrderMatchingEngineConfig>,
) -> OrderMatchingEngine {
    let cache = cache.unwrap_or(Rc::new(RefCell::new(Cache::default())));
    let config = config.unwrap_or_default();
    OrderMatchingEngine::new(
        instrument,
        1,
        FillModel::default(),
        FeeModelAny::default(),
        BookType::L2_MBP,
        OmsType::Netting,
        account_type.unwrap_or(AccountType::Cash),
        &ATOMIC_TIME,
        msgbus,
        cache,
        config,
    )
}

fn get_order_event_handler_messages(event_handler: ShareableMessageHandler) -> Vec<OrderEventAny> {
    get_saved_messages::<OrderEventAny>(event_handler)
}

// -- TESTS -----------------------------------------------------------------------------------

#[rstest]
fn test_process_order_when_instrument_already_expired(
    mut msgbus: MessageBus,
    order_event_handler: ShareableMessageHandler,
    account_id: AccountId,
    mut market_order_buy: OrderAny,
) {
    let instrument = InstrumentAny::FuturesContract(futures_contract_es(None, None));

    // Register saving message handler to exec engine endpoint
    msgbus.register(
        msgbus.switchboard.exec_engine_process,
        order_event_handler.clone(),
    );

    // Create engine and process order
    let mut engine =
        get_order_matching_engine(instrument, Rc::new(RefCell::new(msgbus)), None, None, None);

    engine.process_order(&mut market_order_buy, account_id);

    // Get messages and test
    let saved_messages = get_order_event_handler_messages(order_event_handler);
    assert_eq!(saved_messages.len(), 1);
    let first_message = saved_messages.first().unwrap();
    assert_eq!(first_message.event_type(), OrderEventType::Rejected);
    assert_eq!(
        first_message.message().unwrap(),
        Ustr::from("Contract ESZ21.GLBX has expired, expiration 1639699200000000000")
    );
}

#[rstest]
fn test_process_order_when_instrument_not_active(
    mut msgbus: MessageBus,
    order_event_handler: ShareableMessageHandler,
    account_id: AccountId,
    mut market_order_buy: OrderAny,
) {
    let activation = UnixNanos::from(
        Utc.with_ymd_and_hms(2222, 4, 8, 0, 0, 0)
            .unwrap()
            .timestamp_nanos_opt()
            .unwrap() as u64,
    );
    let expiration = UnixNanos::from(
        Utc.with_ymd_and_hms(2223, 7, 8, 0, 0, 0)
            .unwrap()
            .timestamp_nanos_opt()
            .unwrap() as u64,
    );
    let instrument =
        InstrumentAny::FuturesContract(futures_contract_es(Some(activation), Some(expiration)));

    // Register saving message handler to exec engine endpoint
    msgbus.register(
        msgbus.switchboard.exec_engine_process,
        order_event_handler.clone(),
    );

    // Create engine and process order
    let mut engine =
        get_order_matching_engine(instrument, Rc::new(RefCell::new(msgbus)), None, None, None);

    engine.process_order(&mut market_order_buy, account_id);

    // Get messages and test
    let saved_messages = get_order_event_handler_messages(order_event_handler);
    assert_eq!(saved_messages.len(), 1);
    let first_message = saved_messages.first().unwrap();
    assert_eq!(first_message.event_type(), OrderEventType::Rejected);
    assert_eq!(
        first_message.message().unwrap(),
        Ustr::from("Contract ESZ21.GLBX is not yet active, activation 7960723200000000000")
    );
}

#[rstest]
fn test_process_order_when_invalid_quantity_precision(
    mut msgbus: MessageBus,
    order_event_handler: ShareableMessageHandler,
    account_id: AccountId,
    instrument_eth_usdt: InstrumentAny,
) {
    // Register saving message handler to exec engine endpoint
    msgbus.register(
        msgbus.switchboard.exec_engine_process,
        order_event_handler.clone(),
    );

    // Create market order with invalid quantity precision 0 for eth/usdt precision of 3
    let mut market_order_invalid_precision = OrderTestBuilder::new(OrderType::Market)
        .instrument_id(instrument_eth_usdt.id())
        .side(OrderSide::Buy)
        .quantity(Quantity::from("1"))
        .build();

    // Create engine and process order
    let mut engine = get_order_matching_engine(
        instrument_eth_usdt,
        Rc::new(RefCell::new(msgbus)),
        None,
        None,
        None,
    );

    engine.process_order(&mut market_order_invalid_precision, account_id);

    // Get messages and test
    let saved_messages = get_order_event_handler_messages(order_event_handler);
    assert_eq!(saved_messages.len(), 1);
    let first_message = saved_messages.first().unwrap();
    assert_eq!(first_message.event_type(), OrderEventType::Rejected);
    assert_eq!(
        first_message.message().unwrap(),
        Ustr::from("Invalid order quantity precision for order O-19700101-000000-001-001-1, was 0 when ETHUSDT-PERP.BINANCE size precision is 3")
    );
}

#[rstest]
fn test_process_order_when_invalid_price_precision(
    mut msgbus: MessageBus,
    order_event_handler: ShareableMessageHandler,
    account_id: AccountId,
    instrument_es: InstrumentAny,
) {
    // Register saving message handler to exec engine endpoint
    msgbus.register(
        msgbus.switchboard.exec_engine_process,
        order_event_handler.clone(),
    );

    // Create engine and process order
    let mut engine = get_order_matching_engine(
        instrument_es.clone(),
        Rc::new(RefCell::new(msgbus)),
        None,
        None,
        None,
    );

    let mut limit_order = OrderTestBuilder::new(OrderType::Limit)
        .instrument_id(instrument_es.id())
        .side(OrderSide::Sell)
        .price(Price::from("100.12333")) // <-- Wrong price precision for es futures contract (which is 2)
        .quantity(Quantity::from("1"))
        .build();

    engine.process_order(&mut limit_order, account_id);

    // Get messages and test
    let saved_messages = get_order_event_handler_messages(order_event_handler);
    assert_eq!(saved_messages.len(), 1);
    let first_message = saved_messages.first().unwrap();
    assert_eq!(first_message.event_type(), OrderEventType::Rejected);
    assert_eq!(
        first_message.message().unwrap(),
        Ustr::from("Invalid order price precision for order O-19700101-000000-001-001-1, was 5 when ESZ21.GLBX price precision is 2")
    );
}

#[rstest]
fn test_process_order_when_invalid_trigger_price_precision(
    mut msgbus: MessageBus,
    order_event_handler: ShareableMessageHandler,
    account_id: AccountId,
    instrument_es: InstrumentAny,
) {
    // Register saving message handler to exec engine endpoint
    msgbus.register(
        msgbus.switchboard.exec_engine_process,
        order_event_handler.clone(),
    );

    // Create engine and process order
    let mut engine = get_order_matching_engine(
        instrument_es.clone(),
        Rc::new(RefCell::new(msgbus)),
        None,
        None,
        None,
    );
    let mut stop_order = OrderTestBuilder::new(OrderType::StopMarket)
        .instrument_id(instrument_es.id())
        .side(OrderSide::Sell)
        .trigger_price(Price::from("100.12333")) // <-- Wrong trigger price precision for es futures contract (which is 2)
        .quantity(Quantity::from("1"))
        .build();

    engine.process_order(&mut stop_order, account_id);

    // Get messages and test
    let saved_messages = get_order_event_handler_messages(order_event_handler);
    assert_eq!(saved_messages.len(), 1);
    let first_message = saved_messages.first().unwrap();
    assert_eq!(first_message.event_type(), OrderEventType::Rejected);
    assert_eq!(
        first_message.message().unwrap(),
        Ustr::from("Invalid order trigger price precision for order O-19700101-000000-001-001-1, was 5 when ESZ21.GLBX price precision is 2")
    );
}

#[rstest]
fn test_process_order_when_shorting_equity_without_margin_account(
    mut msgbus: MessageBus,
    order_event_handler: ShareableMessageHandler,
    account_id: AccountId,
    equity_aapl: Equity,
) {
    let instrument = InstrumentAny::Equity(equity_aapl);
    // Register saving message handler to exec engine endpoint
    msgbus.register(
        msgbus.switchboard.exec_engine_process,
        order_event_handler.clone(),
    );

    let mut market_order_sell = OrderTestBuilder::new(OrderType::Market)
        .instrument_id(instrument.id())
        .side(OrderSide::Sell)
        .quantity(Quantity::from("1"))
        .build();

    // Create engine and process order
    let mut engine =
        get_order_matching_engine(instrument, Rc::new(RefCell::new(msgbus)), None, None, None);

    engine.process_order(&mut market_order_sell, account_id);

    // Get messages and test
    let saved_messages = get_order_event_handler_messages(order_event_handler);
    assert_eq!(saved_messages.len(), 1);
    let first_message = saved_messages.first().unwrap();
    assert_eq!(first_message.event_type(), OrderEventType::Rejected);
    assert_eq!(
        first_message.message().unwrap(),
        Ustr::from(
            "Short selling not permitted on a CASH account with position None and \
            order MarketOrder(SELL 1 AAPL.XNAS @ MARKET GTC, status=INITIALIZED, \
            client_order_id=O-19700101-000000-001-001-1, venue_order_id=None, position_id=None, \
            exec_algorithm_id=None, exec_spawn_id=None, tags=None)"
        )
    );
}

#[rstest]
fn test_process_order_when_invalid_reduce_only(
    mut msgbus: MessageBus,
    order_event_handler: ShareableMessageHandler,
    account_id: AccountId,
    instrument_eth_usdt: InstrumentAny,
    engine_config: OrderMatchingEngineConfig,
) {
    // Register saving message handler to exec engine endpoint
    msgbus.register(
        msgbus.switchboard.exec_engine_process,
        order_event_handler.clone(),
    );

    let mut engine = get_order_matching_engine(
        instrument_eth_usdt.clone(),
        Rc::new(RefCell::new(msgbus)),
        None,
        None,
        Some(engine_config),
    );
    let mut market_order_reduce = OrderTestBuilder::new(OrderType::Market)
        .instrument_id(instrument_eth_usdt.id())
        .side(OrderSide::Buy)
        .quantity(Quantity::from("1.000"))
        .reduce_only(true)
        .build();

    engine.process_order(&mut market_order_reduce, account_id);

    // Get messages and test
    let saved_messages = get_order_event_handler_messages(order_event_handler);
    assert_eq!(saved_messages.len(), 1);
    let first_message = saved_messages.first().unwrap();
    assert_eq!(first_message.event_type(), OrderEventType::Rejected);
    assert_eq!(
        first_message.message().unwrap(),
        Ustr::from("Reduce-only order O-19700101-000000-001-001-1 (MARKET-BUY) would have increased position")
    );
}

#[rstest]
fn test_process_order_when_invalid_contingent_orders(
    mut msgbus: MessageBus,
    order_event_handler: ShareableMessageHandler,
    account_id: AccountId,
    instrument_es: InstrumentAny,
    engine_config: OrderMatchingEngineConfig,
) {
    // Register saving message handler to exec engine endpoint
    msgbus.register(
        msgbus.switchboard.exec_engine_process,
        order_event_handler.clone(),
    );

    let cache = Rc::new(RefCell::new(Cache::default()));
    let mut engine = get_order_matching_engine(
        instrument_es.clone(),
        Rc::new(RefCell::new(msgbus)),
        Some(cache.clone()),
        None,
        Some(engine_config),
    );

    let entry_client_order_id = ClientOrderId::from("O-19700101-000000-001-001-1");
    let stop_loss_client_order_id = ClientOrderId::from("O-19700101-000000-001-001-2");

    // Create entry market order
    let mut entry_order = OrderTestBuilder::new(OrderType::Market)
        .instrument_id(instrument_es.id())
        .side(OrderSide::Buy)
        .quantity(Quantity::from(1))
        .contingency_type(ContingencyType::Oto)
        .client_order_id(entry_client_order_id)
        .build();
    // Set entry order status to Rejected with proper event
    let rejected_event = OrderRejected::default();
    entry_order
        .apply(OrderEventAny::Rejected(rejected_event))
        .unwrap();

    // Create stop loss order
    let stop_order = OrderTestBuilder::new(OrderType::StopMarket)
        .instrument_id(instrument_es.id())
        .side(OrderSide::Sell)
        .trigger_price(Price::from("0.95"))
        .quantity(Quantity::from(1))
        .contingency_type(ContingencyType::Oto)
        .client_order_id(stop_loss_client_order_id)
        .parent_order_id(entry_client_order_id)
        .build();
    // Make it Accepted
    let mut accepted_stop_order = TestOrderStubs::make_accepted_order(&stop_order);

    // 1. Save entry order in the cache as it will be loaded by the matching engine
    // 2. Send the stop loss order which has parent of entry order
    cache
        .as_ref()
        .borrow_mut()
        .add_order(entry_order.clone(), None, None, false)
        .unwrap();
    engine.process_order(&mut accepted_stop_order, account_id);

    // Get messages and test
    let saved_messages = get_order_event_handler_messages(order_event_handler);
    assert_eq!(saved_messages.len(), 1);
    let first_message = saved_messages.first().unwrap();
    assert_eq!(first_message.event_type(), OrderEventType::Rejected);
    assert_eq!(
        first_message.message().unwrap(),
        Ustr::from(format!("Rejected OTO order from {entry_client_order_id}").as_str())
    );
}

#[rstest]
fn test_process_order_when_closed_linked_order(
    mut msgbus: MessageBus,
    order_event_handler: ShareableMessageHandler,
    account_id: AccountId,
    instrument_es: InstrumentAny,
    engine_config: OrderMatchingEngineConfig,
) {
    // Register saving message handler to exec engine endpoint
    msgbus.register(
        msgbus.switchboard.exec_engine_process,
        order_event_handler.clone(),
    );

    let cache = Rc::new(RefCell::new(Cache::default()));
    let mut engine = get_order_matching_engine(
        instrument_es.clone(),
        Rc::new(RefCell::new(msgbus)),
        Some(cache.clone()),
        None,
        Some(engine_config),
    );

    let stop_loss_client_order_id = ClientOrderId::from("O-19700101-000000-001-001-2");
    let take_profit_client_order_id = ClientOrderId::from("O-19700101-000000-001-001-3");
    // Create two linked orders: stop loss and take profit
    let mut stop_loss_order = OrderTestBuilder::new(OrderType::StopMarket)
        .instrument_id(instrument_es.id())
        .side(OrderSide::Sell)
        .trigger_price(Price::from("0.95"))
        .quantity(Quantity::from(1))
        .contingency_type(ContingencyType::Oco)
        .client_order_id(stop_loss_client_order_id)
        .linked_order_ids(vec![take_profit_client_order_id])
        .build();
    let take_profit_order = OrderTestBuilder::new(OrderType::MarketIfTouched)
        .instrument_id(instrument_es.id())
        .side(OrderSide::Sell)
        .trigger_price(Price::from("1.1"))
        .quantity(Quantity::from(1))
        .contingency_type(ContingencyType::Oco)
        .client_order_id(take_profit_client_order_id)
        .linked_order_ids(vec![stop_loss_client_order_id])
        .build();
    // Set stop loss order status to Rejected with proper event
    let rejected_event: OrderRejected = OrderRejectedBuilder::default()
        .client_order_id(stop_loss_client_order_id)
        .reason(Ustr::from("Rejected"))
        .build()
        .unwrap();
    stop_loss_order
        .apply(OrderEventAny::Rejected(rejected_event))
        .unwrap();

    // Make take profit order Accepted
    let mut accepted_take_profit = TestOrderStubs::make_accepted_order(&take_profit_order);

    // 1. Save stop loss order in cache which is rejected and closed is set to true
    // 2. Send the take profit order which has linked the stop loss order
    cache
        .as_ref()
        .borrow_mut()
        .add_order(stop_loss_order.clone(), None, None, false)
        .unwrap();
    let _stop_loss_closed_after = stop_loss_order.is_closed();
    engine.process_order(&mut accepted_take_profit, account_id);

    // Get messages and test
    let saved_messages = get_order_event_handler_messages(order_event_handler);
    assert_eq!(saved_messages.len(), 1);
    let first_message = saved_messages.first().unwrap();
    assert_eq!(first_message.event_type(), OrderEventType::Rejected);
    assert_eq!(
        first_message.message().unwrap(),
        Ustr::from(format!("Contingent order {stop_loss_client_order_id} already closed").as_str())
    );
}

#[rstest]
fn test_process_market_order_no_market_rejected(
    mut msgbus: MessageBus,
    order_event_handler: ShareableMessageHandler,
    account_id: AccountId,
    instrument_eth_usdt: InstrumentAny,
    mut market_order_buy: OrderAny,
    mut market_order_sell: OrderAny,
) {
    // Register saving message handler to exec engine endpoint
    msgbus.register(
        msgbus.switchboard.exec_engine_process,
        order_event_handler.clone(),
    );

    // Create engine and process order
    let mut engine = get_order_matching_engine(
        instrument_eth_usdt,
        Rc::new(RefCell::new(msgbus)),
        None,
        None,
        None,
    );

    engine.process_order(&mut market_order_buy, account_id);
    engine.process_order(&mut market_order_sell, account_id);

    // Get messages and test
    let saved_messages = get_order_event_handler_messages(order_event_handler);
    assert_eq!(saved_messages.len(), 2);
    let first = saved_messages.first().unwrap();
    let second = saved_messages.get(1).unwrap();
    assert_eq!(first.event_type(), OrderEventType::Rejected);
    assert_eq!(second.event_type(), OrderEventType::Rejected);
    assert_eq!(
        first.message().unwrap(),
        Ustr::from("No market for ETHUSDT-PERP.BINANCE")
    );
    assert_eq!(
        second.message().unwrap(),
        Ustr::from("No market for ETHUSDT-PERP.BINANCE")
    );
}

#[rstest]
fn test_bid_ask_initialized(msgbus: MessageBus, instrument_es: InstrumentAny) {
    let mut engine_l2 = get_order_matching_engine_l2(
        instrument_es.clone(),
        Rc::new(RefCell::new(msgbus)),
        None,
        None,
        None,
    );
    // Create bid and ask orderbook delta and check if
    // bid and ask are initialized in order matching core
    let book_order_buy = BookOrder::new(OrderSide::Buy, Price::from("100"), Quantity::from("1"), 0);
    let book_order_sell =
        BookOrder::new(OrderSide::Sell, Price::from("101"), Quantity::from("1"), 0);
    let orderbook_delta_buy = OrderBookDeltaTestBuilder::new(instrument_es.id())
        .book_action(BookAction::Add)
        .book_order(book_order_buy)
        .build();
    let orderbook_delta_sell = OrderBookDeltaTestBuilder::new(instrument_es.id())
        .book_action(BookAction::Add)
        .book_order(book_order_sell)
        .build();

    engine_l2.process_order_book_delta(&orderbook_delta_buy);
    assert_eq!(engine_l2.core.bid, Some(Price::from("100")));
    assert!(engine_l2.core.is_bid_initialized);
    assert_eq!(engine_l2.core.ask, None);
    assert!(!engine_l2.core.is_ask_initialized);

    engine_l2.process_order_book_delta(&orderbook_delta_sell);
    assert_eq!(engine_l2.core.bid, Some(Price::from("100")));
    assert!(engine_l2.core.is_bid_initialized);
    assert_eq!(engine_l2.core.ask, Some(Price::from("101")));
    assert!(engine_l2.core.is_ask_initialized);
}

#[rstest]
fn test_not_enough_quantity_filled_fok_order(
    instrument_eth_usdt: InstrumentAny,
    order_event_handler: ShareableMessageHandler,
    mut msgbus: MessageBus,
    account_id: AccountId,
) {
    msgbus.register(
        msgbus.switchboard.exec_engine_process,
        order_event_handler.clone(),
    );
    let mut engine_l2 = get_order_matching_engine_l2(
        instrument_eth_usdt.clone(),
        Rc::new(RefCell::new(msgbus)),
        None,
        None,
        None,
    );

    let orderbook_delta_sell = OrderBookDeltaTestBuilder::new(instrument_eth_usdt.id())
        .book_action(BookAction::Add)
        .book_order(BookOrder::new(
            OrderSide::Sell,
            Price::from("1500.00"),
            Quantity::from("1.000"),
            1,
        ))
        .build();

    // Create FOK market order with quantity 2 which wont be enough to fill the order
    let mut market_order = OrderTestBuilder::new(OrderType::Market)
        .instrument_id(instrument_eth_usdt.id())
        .side(OrderSide::Buy)
        .quantity(Quantity::from("2.000"))
        .client_order_id(ClientOrderId::from("O-19700101-000000-001-001-1"))
        .time_in_force(TimeInForce::Fok)
        .build();

    engine_l2.process_order_book_delta(&orderbook_delta_sell);
    engine_l2.process_order(&mut market_order, account_id);

    // We need to test that one Order rejected event was generated
    let saved_messages = get_order_event_handler_messages(order_event_handler);
    assert_eq!(saved_messages.len(), 1);
    let first_message = saved_messages.first().unwrap();
    assert_eq!(first_message.event_type(), OrderEventType::Rejected);
    let order_rejected = match first_message {
        OrderEventAny::Rejected(order_rejected) => order_rejected,
        _ => panic!("Expected OrderRejected event in first message"),
    };
    assert_eq!(
        order_rejected.reason,
        Ustr::from("Fill or kill order cannot be filled at full amount")
    );
}

#[rstest]
fn test_valid_market_buy(
    instrument_eth_usdt: InstrumentAny,
    order_event_handler: ShareableMessageHandler,
    mut msgbus: MessageBus,
    account_id: AccountId,
) {
    msgbus.register(
        msgbus.switchboard.exec_engine_process,
        order_event_handler.clone(),
    );
    let mut engine_l2 = get_order_matching_engine_l2(
        instrument_eth_usdt.clone(),
        Rc::new(RefCell::new(msgbus)),
        None,
        None,
        None,
    );

    // Create 2 orderbook deltas and appropriate market order
    let book_order_1 = BookOrder::new(
        OrderSide::Sell,
        Price::from("1500.00"),
        Quantity::from("1.000"),
        1,
    );
    let book_order_2 = BookOrder::new(
        OrderSide::Sell,
        Price::from("1510.00"),
        Quantity::from("1.000"),
        1,
    );
    let orderbook_delta_sell_1 = OrderBookDeltaTestBuilder::new(instrument_eth_usdt.id())
        .book_action(BookAction::Add)
        .book_order(book_order_1)
        .build();
    let orderbook_delta_sell_2 = OrderBookDeltaTestBuilder::new(instrument_eth_usdt.id())
        .book_action(BookAction::Add)
        .book_order(book_order_2)
        .build();

    let mut market_order = OrderTestBuilder::new(OrderType::Market)
        .instrument_id(instrument_eth_usdt.id())
        .side(OrderSide::Buy)
        .quantity(Quantity::from("2.000"))
        .client_order_id(ClientOrderId::from("O-19700101-000000-001-001-1"))
        .build();

    // Process orderbook deltas to add liquidity then process market order
    engine_l2.process_order_book_delta(&orderbook_delta_sell_1);
    engine_l2.process_order_book_delta(&orderbook_delta_sell_2);
    engine_l2.process_order(&mut market_order, account_id);

    // We need to test that two Order filled events were generated where with correct prices and quantities
    let saved_messages = get_order_event_handler_messages(order_event_handler);
    assert_eq!(saved_messages.len(), 2);
    let order_event_first = saved_messages.first().unwrap();
    let order_filled_first = match order_event_first {
        OrderEventAny::Filled(order_filled) => order_filled,
        _ => panic!("Expected OrderFilled event in first message"),
    };
    let order_event_second = saved_messages.get(1).unwrap();
    let order_filled_second = match order_event_second {
        OrderEventAny::Filled(order_filled) => order_filled,
        _ => panic!("Expected OrderFilled event in second message"),
    };
    // Check correct prices and quantities
    assert_eq!(order_filled_first.last_px, Price::from("1500.00"));
    assert_eq!(order_filled_first.last_qty, Quantity::from("1.000"));
    assert_eq!(order_filled_second.last_px, Price::from("1510.00"));
    assert_eq!(order_filled_second.last_qty, Quantity::from("1.000"));
}

#[rstest]
fn test_process_limit_post_only_order_that_would_be_a_taker(
    instrument_eth_usdt: InstrumentAny,
    mut msgbus: MessageBus,
    order_event_handler: ShareableMessageHandler,
    account_id: AccountId,
) {
    // Register saving message handler to exec engine endpoint
    msgbus.register(
        msgbus.switchboard.exec_engine_process,
        order_event_handler.clone(),
    );

    let mut engine_l2 = get_order_matching_engine_l2(
        instrument_eth_usdt.clone(),
        Rc::new(RefCell::new(msgbus)),
        None,
        None,
        None,
    );

    let orderbook_delta_sell = OrderBookDeltaTestBuilder::new(instrument_eth_usdt.id())
        .book_action(BookAction::Add)
        .book_order(BookOrder::new(
            OrderSide::Sell,
            Price::from("1500.00"),
            Quantity::from("1.000"),
            1,
        ))
        .build();

    // Create a post-only limit buy order with price above 1500.00
    // that would match the existing sell order and be a taker
    let mut post_only_limit_order = OrderTestBuilder::new(OrderType::Limit)
        .instrument_id(instrument_eth_usdt.id())
        .side(OrderSide::Buy)
        .price(Price::from("1501.00"))
        .quantity(Quantity::from("1.000"))
        .post_only(true)
        .client_order_id(ClientOrderId::from("O-19700101-000000-001-001-1"))
        .build();

    engine_l2.process_order_book_delta(&orderbook_delta_sell);
    engine_l2.process_order(&mut post_only_limit_order, account_id);

    // Test that one Order rejected event was generated
    let saved_messages = get_order_event_handler_messages(order_event_handler);
    assert_eq!(saved_messages.len(), 1);
    let first_message = saved_messages.first().unwrap();
    assert_eq!(first_message.event_type(), OrderEventType::Rejected);
    let order_rejected = match first_message {
        OrderEventAny::Rejected(order_rejected) => order_rejected,
        _ => panic!("Expected OrderRejected event in first message"),
    };
    assert_eq!(
        order_rejected.reason,
        Ustr::from("POST_ONLY LIMIT BUY order limit px of 1501.00 would have been a TAKER: bid=None, ask=1500.00")
    );
}

#[rstest]
fn test_process_limit_order_not_matched_and_canceled_fok_order(
    instrument_eth_usdt: InstrumentAny,
    mut msgbus: MessageBus,
    order_event_handler: ShareableMessageHandler,
    account_id: AccountId,
) {
    // Register saving message handler to exec engine endpoint
    msgbus.register(
        msgbus.switchboard.exec_engine_process,
        order_event_handler.clone(),
    );

    let mut engine_l2 = get_order_matching_engine_l2(
        instrument_eth_usdt.clone(),
        Rc::new(RefCell::new(msgbus)),
        None,
        None,
        None,
    );

    let orderbook_delta_sell = OrderBookDeltaTestBuilder::new(instrument_eth_usdt.id())
        .book_action(BookAction::Add)
        .book_order(BookOrder::new(
            OrderSide::Sell,
            Price::from("1500.00"),
            Quantity::from("1.000"),
            1,
        ))
        .build();

    let client_order_id = ClientOrderId::from("O-19700101-000000-001-001-1");
    // Create limit order which is bellow currently supplied liquidity and ask
    let mut limit_order = OrderTestBuilder::new(OrderType::Limit)
        .instrument_id(instrument_eth_usdt.id())
        .side(OrderSide::Buy)
        .price(Price::from("1495.00"))
        .quantity(Quantity::from("1.000"))
        .time_in_force(TimeInForce::Fok)
        .client_order_id(client_order_id)
        .build();

    engine_l2.process_order_book_delta(&orderbook_delta_sell);
    engine_l2.process_order(&mut limit_order, account_id);

    // Check we have received OrderAccepted and then OrderCanceled event
    let saved_messages = get_order_event_handler_messages(order_event_handler);
    assert_eq!(saved_messages.len(), 2);
    let order_event_first = saved_messages.first().unwrap();
    let order_accepted = match order_event_first {
        OrderEventAny::Accepted(order_accepted) => order_accepted,
        _ => panic!("Expected OrderAccepted event in first message"),
    };
    assert_eq!(order_accepted.client_order_id, client_order_id);
    let order_event_second = saved_messages.get(1).unwrap();
    let order_rejected = match order_event_second {
        OrderEventAny::Canceled(order_canceled) => order_canceled,
        _ => panic!("Expected OrderCanceled event in second message"),
    };
    assert_eq!(order_rejected.client_order_id, client_order_id);
}

#[rstest]
fn test_process_limit_order_matched_immediate_fill(
    instrument_eth_usdt: InstrumentAny,
    mut msgbus: MessageBus,
    order_event_handler: ShareableMessageHandler,
    account_id: AccountId,
) {
    // Register saving message handler to exec engine endpoint
    msgbus.register(
        msgbus.switchboard.exec_engine_process,
        order_event_handler.clone(),
    );

    let mut engine_l2 = get_order_matching_engine_l2(
        instrument_eth_usdt.clone(),
        Rc::new(RefCell::new(msgbus)),
        None,
        None,
        None,
    );

    let orderbook_delta_sell = OrderBookDeltaTestBuilder::new(instrument_eth_usdt.id())
        .book_action(BookAction::Add)
        .book_order(BookOrder::new(
            OrderSide::Sell,
            Price::from("1500.00"),
            Quantity::from("1.000"),
            1,
        ))
        .build();
    let client_order_id = ClientOrderId::from("O-19700101-000000-001-001-1");
    let mut limit_order = OrderTestBuilder::new(OrderType::Limit)
        .instrument_id(instrument_eth_usdt.id())
        .side(OrderSide::Buy)
        .price(Price::from("1501.00"))
        .quantity(Quantity::from("1.000"))
        .client_order_id(client_order_id)
        .build();

    engine_l2.process_order_book_delta(&orderbook_delta_sell);
    engine_l2.process_order(&mut limit_order, account_id);

    // Check we have received first OrderAccepted and then OrderFilled event
    let saved_messages = get_order_event_handler_messages(order_event_handler);
    assert_eq!(saved_messages.len(), 2);
    let order_event_first = saved_messages.first().unwrap();
    let order_accepted = match order_event_first {
        OrderEventAny::Accepted(order_accepted) => order_accepted,
        _ => panic!("Expected OrderAccepted event in first message"),
    };
    assert_eq!(order_accepted.client_order_id, client_order_id);
    let order_event_second = saved_messages.get(1).unwrap();
    let order_filled = match order_event_second {
        OrderEventAny::Filled(order_filled) => order_filled,
        _ => panic!("Expected OrderFilled event in second message"),
    };
    assert_eq!(order_filled.client_order_id, client_order_id);
    assert_eq!(order_filled.last_px, Price::from("1500.00"));
    assert_eq!(order_filled.last_qty, Quantity::from("1.000"));
}

#[rstest]
fn test_process_stop_market_order_triggered_rejected(
    instrument_eth_usdt: InstrumentAny,
    mut msgbus: MessageBus,
    order_event_handler: ShareableMessageHandler,
    account_id: AccountId,
) {
    // Register saving message handler to exec engine endpoint
    msgbus.register(
        msgbus.switchboard.exec_engine_process,
        order_event_handler.clone(),
    );

    // Create order matching engine which rejects stop orders
    let mut engine_config = OrderMatchingEngineConfig::default();
    engine_config.reject_stop_orders = true;
    let mut engine_l2 = get_order_matching_engine_l2(
        instrument_eth_usdt.clone(),
        Rc::new(RefCell::new(msgbus)),
        None,
        None,
        Some(engine_config),
    );

    let orderbook_delta_sell = OrderBookDeltaTestBuilder::new(instrument_eth_usdt.id())
        .book_action(BookAction::Add)
        .book_order(BookOrder::new(
            OrderSide::Sell,
            Price::from("1500.00"),
            Quantity::from("1.000"),
            1,
        ))
        .build();
    let client_order_id = ClientOrderId::from("O-19700101-000000-001-001-1");
    // Create but stop market order, which is triggered (price of 1495 is below current ask of 1500)
    let mut stop_order = OrderTestBuilder::new(OrderType::StopMarket)
        .instrument_id(instrument_eth_usdt.id())
        .side(OrderSide::Buy)
        .trigger_price(Price::from("1495.00"))
        .quantity(Quantity::from("1.000"))
        .client_order_id(client_order_id)
        .build();

    engine_l2.process_order_book_delta(&orderbook_delta_sell);
    engine_l2.process_order(&mut stop_order, account_id);

    // Check we have received OrderRejected event
    let saved_messages = get_order_event_handler_messages(order_event_handler);
    assert_eq!(saved_messages.len(), 1);
    let order_event = saved_messages.first().unwrap();
    let order_rejected = match order_event {
        OrderEventAny::Rejected(order_rejected) => order_rejected,
        _ => panic!("Expected OrderRejected event in first message"),
    };
    assert_eq!(order_rejected.client_order_id, client_order_id);
    assert_eq!(
        order_rejected.reason,
        Ustr::from("STOP_MARKET BUY order stop px of 1495.00 was in the market: bid=None, ask=1500.00, but rejected because of configuration")
    );
}

#[rstest]
fn test_process_stop_market_order_valid_trigger_filled(
    instrument_eth_usdt: InstrumentAny,
    mut msgbus: MessageBus,
    order_event_handler: ShareableMessageHandler,
    account_id: AccountId,
) {
    msgbus.register(
        msgbus.switchboard.exec_engine_process,
        order_event_handler.clone(),
    );
    // Create normal l2 engine without reject_stop_orders config param
    let mut engine_l2 = get_order_matching_engine_l2(
        instrument_eth_usdt.clone(),
        Rc::new(RefCell::new(msgbus)),
        None,
        None,
        None,
    );

    let orderbook_delta_sell = OrderBookDeltaTestBuilder::new(instrument_eth_usdt.id())
        .book_action(BookAction::Add)
        .book_order(BookOrder::new(
            OrderSide::Sell,
            Price::from("1500.00"),
            Quantity::from("1.000"),
            1,
        ))
        .build();
    let client_order_id = ClientOrderId::from("O-19700101-000000-001-001-1");
    // Create but stop market order, which is triggered (price of 1495 is below current ask of 1500)
    let mut stop_order = OrderTestBuilder::new(OrderType::StopMarket)
        .instrument_id(instrument_eth_usdt.id())
        .side(OrderSide::Buy)
        .trigger_price(Price::from("1495.00"))
        .quantity(Quantity::from("1.000"))
        .client_order_id(client_order_id)
        .build();

    engine_l2.process_order_book_delta(&orderbook_delta_sell);
    engine_l2.process_order(&mut stop_order, account_id);

    // Check we have received OrderFilled event
    let saved_messages = get_order_event_handler_messages(order_event_handler);
    assert_eq!(saved_messages.len(), 1);
    let order_filled = saved_messages.first().unwrap();
    let order_filled = match order_filled {
        OrderEventAny::Filled(order_filled) => order_filled,
        _ => panic!("Expected OrderFilled event in first message"),
    };
    assert_eq!(order_filled.client_order_id, client_order_id);
    assert_eq!(order_filled.last_px, Price::from("1500.00"));
    assert_eq!(order_filled.last_qty, Quantity::from("1.000"));
}

#[rstest]
fn test_process_stop_market_order_valid_not_triggered_accepted(
    instrument_eth_usdt: InstrumentAny,
    mut msgbus: MessageBus,
    order_event_handler: ShareableMessageHandler,
    account_id: AccountId,
) {
    msgbus.register(
        msgbus.switchboard.exec_engine_process,
        order_event_handler.clone(),
    );
    let mut engine_l2 = get_order_matching_engine_l2(
        instrument_eth_usdt.clone(),
        Rc::new(RefCell::new(msgbus)),
        None,
        None,
        None,
    );

    let orderbook_delta_sell = OrderBookDeltaTestBuilder::new(instrument_eth_usdt.id())
        .book_action(BookAction::Add)
        .book_order(BookOrder::new(
            OrderSide::Sell,
            Price::from("1500.00"),
            Quantity::from("1.000"),
            1,
        ))
        .build();
    let client_order_id = ClientOrderId::from("O-19700101-000000-001-001-1");
    // Create but stop market order, which is not triggered (price of 1505 is above current ask of 1500)
    let mut stop_order = OrderTestBuilder::new(OrderType::StopMarket)
        .instrument_id(instrument_eth_usdt.id())
        .side(OrderSide::Buy)
        .trigger_price(Price::from("1505.00"))
        .quantity(Quantity::from("1.000"))
        .client_order_id(client_order_id)
        .build();

    engine_l2.process_order_book_delta(&orderbook_delta_sell);
    engine_l2.process_order(&mut stop_order, account_id);

    // Check we have received OrderAccepted event
    let saved_messages = get_order_event_handler_messages(order_event_handler);
    assert_eq!(saved_messages.len(), 1);
    let order_event = saved_messages.first().unwrap();
    let order_accepted = match order_event {
        OrderEventAny::Accepted(order_accepted) => order_accepted,
        _ => panic!("Expected OrderAccepted event in first message"),
    };
    assert_eq!(order_accepted.client_order_id, client_order_id);
}

#[rstest]
fn test_process_stop_limit_order_triggered_not_filled(
    instrument_eth_usdt: InstrumentAny,
    mut msgbus: MessageBus,
    order_event_handler: ShareableMessageHandler,
    account_id: AccountId,
) {
    msgbus.register(
        msgbus.switchboard.exec_engine_process,
        order_event_handler.clone(),
    );
    let mut engine_l2 = get_order_matching_engine_l2(
        instrument_eth_usdt.clone(),
        Rc::new(RefCell::new(msgbus)),
        None,
        None,
        None,
    );

    let orderbook_delta_sell = OrderBookDeltaTestBuilder::new(instrument_eth_usdt.id())
        .book_action(BookAction::Add)
        .book_order(BookOrder::new(
            OrderSide::Sell,
            Price::from("1500.00"),
            Quantity::from("1.000"),
            1,
        ))
        .build();
    let client_order_id = ClientOrderId::from("O-19700101-000000-001-001-1");
    // Create but stop limit order, which is triggered (price of 1495 is bellow current ask of 1500)
    // but price of 1490 it's not immediately filled.
    let mut stop_order = OrderTestBuilder::new(OrderType::StopLimit)
        .instrument_id(instrument_eth_usdt.id())
        .side(OrderSide::Buy)
        .trigger_price(Price::from("1495.00"))
        .price(Price::from("1490.00"))
        .quantity(Quantity::from("1.000"))
        .client_order_id(client_order_id)
        .build();

    engine_l2.process_order_book_delta(&orderbook_delta_sell);
    engine_l2.process_order(&mut stop_order, account_id);

    // Check we have received OrderAccepted and OrderTriggered
    let saved_messages = get_order_event_handler_messages(order_event_handler);
    assert_eq!(saved_messages.len(), 2);
    let order_event_first = saved_messages.first().unwrap();
    let order_accepted = match order_event_first {
        OrderEventAny::Accepted(order_accepted) => order_accepted,
        _ => panic!("Expected OrderAccepted event in first message"),
    };
    assert_eq!(order_accepted.client_order_id, client_order_id);
    let order_event_second = saved_messages.get(1).unwrap();
    let order_triggered = match order_event_second {
        OrderEventAny::Triggered(order_triggered) => order_triggered,
        _ => panic!("Expected OrderTriggered event in second message"),
    };
    assert_eq!(order_triggered.client_order_id, client_order_id);
}

#[rstest]
fn test_process_stop_limit_order_triggered_filled(
    instrument_eth_usdt: InstrumentAny,
    mut msgbus: MessageBus,
    order_event_handler: ShareableMessageHandler,
    account_id: AccountId,
) {
    msgbus.register(
        msgbus.switchboard.exec_engine_process,
        order_event_handler.clone(),
    );
    // Create normal l2 engine without reject_stop_orders config param
    let mut engine_l2 = get_order_matching_engine_l2(
        instrument_eth_usdt.clone(),
        Rc::new(RefCell::new(msgbus)),
        None,
        None,
        None,
    );

    let orderbook_delta_sell = OrderBookDeltaTestBuilder::new(instrument_eth_usdt.id())
        .book_action(BookAction::Add)
        .book_order(BookOrder::new(
            OrderSide::Sell,
            Price::from("1500.00"),
            Quantity::from("1.000"),
            1,
        ))
        .build();
    let client_order_id = ClientOrderId::from("O-19700101-000000-001-001-1");
    // Create but stop limit order, which is triggered (price of 1505 is above current ask of 1500)
    // and price 1502 is also above current ask of 1500 so it's immediately filled.
    let mut stop_order = OrderTestBuilder::new(OrderType::StopLimit)
        .instrument_id(instrument_eth_usdt.id())
        .side(OrderSide::Buy)
        .trigger_price(Price::from("1495.00"))
        .price(Price::from("1502.00"))
        .quantity(Quantity::from("1.000"))
        .client_order_id(client_order_id)
        .build();

    engine_l2.process_order_book_delta(&orderbook_delta_sell);
    engine_l2.process_order(&mut stop_order, account_id);

    // Check we have received OrderAccepted, OrderTriggered and finally OrderFilled event
    let saved_messages = get_order_event_handler_messages(order_event_handler);
    assert_eq!(saved_messages.len(), 3);
    let order_event_first = saved_messages.first().unwrap();
    let order_accepted = match order_event_first {
        OrderEventAny::Accepted(order_accepted) => order_accepted,
        _ => panic!("Expected OrderAccepted event in first message"),
    };
    assert_eq!(order_accepted.client_order_id, client_order_id);
    let order_event_second = saved_messages.get(1).unwrap();
    let order_triggered = match order_event_second {
        OrderEventAny::Triggered(order_triggered) => order_triggered,
        _ => panic!("Expected OrderTriggered event in second message"),
    };
    assert_eq!(order_triggered.client_order_id, client_order_id);
    let order_event_third = saved_messages.get(2).unwrap();
    let order_filled = match order_event_third {
        OrderEventAny::Filled(order_filled) => order_filled,
        _ => panic!("Expected OrderFilled event in third message"),
    };
    assert_eq!(order_filled.client_order_id, client_order_id);
    assert_eq!(order_filled.last_px, Price::from("1500.00"));
    assert_eq!(order_filled.last_qty, Quantity::from("1.000"));
}

#[rstest]
fn test_process_cancel_command_valid(
    instrument_eth_usdt: InstrumentAny,
    mut msgbus: MessageBus,
    order_event_handler: ShareableMessageHandler,
    account_id: AccountId,
) {
    msgbus.register(
        msgbus.switchboard.exec_engine_process,
        order_event_handler.clone(),
    );
    // Create normal l2 engine without reject_stop_orders config param
    let mut engine_l2 = get_order_matching_engine_l2(
        instrument_eth_usdt.clone(),
        Rc::new(RefCell::new(msgbus)),
        None,
        None,
        None,
    );

    let orderbook_delta_sell = OrderBookDeltaTestBuilder::new(instrument_eth_usdt.id())
        .book_action(BookAction::Add)
        .book_order(BookOrder::new(
            OrderSide::Sell,
            Price::from("1500.00"),
            Quantity::from("1.000"),
            1,
        ))
        .build();
    let client_order_id = ClientOrderId::from("O-19700101-000000-001-001-1");
    // Create BUY LIMIT order bellow current ask, so it wont be filled
    let mut limit_order = OrderTestBuilder::new(OrderType::Limit)
        .instrument_id(instrument_eth_usdt.id())
        .side(OrderSide::Buy)
        .price(Price::from("1495.00"))
        .quantity(Quantity::from("1.000"))
        .client_order_id(client_order_id)
        .build();
    // Create cancel command for limit order above
    let cancel_command = CancelOrder::new(
        TraderId::from("TRADER-001"),
        ClientId::from("CLIENT-001"),
        StrategyId::from("STRATEGY-001"),
        instrument_eth_usdt.id(),
        client_order_id,
        VenueOrderId::from("V1"),
        UUID4::new(),
        UnixNanos::default(),
    )
    .unwrap();

    engine_l2.process_order_book_delta(&orderbook_delta_sell);
    engine_l2.process_order(&mut limit_order, account_id);
    engine_l2.process_cancel(&cancel_command, account_id);

    // Check we have received OrderAccepted and then OrderCanceled event
    let saved_messages = get_order_event_handler_messages(order_event_handler);
    assert_eq!(saved_messages.len(), 2);
    let order_event_first = saved_messages.first().unwrap();
    let order_accepted = match order_event_first {
        OrderEventAny::Accepted(order_accepted) => order_accepted,
        _ => panic!("Expected OrderAccepted event in first message"),
    };
    assert_eq!(order_accepted.client_order_id, client_order_id);
    let order_event_second = saved_messages.get(1).unwrap();
    let order_canceled = match order_event_second {
        OrderEventAny::Canceled(order_canceled) => order_canceled,
        _ => panic!("Expected OrderCanceled event in second message"),
    };
    assert_eq!(order_canceled.client_order_id, client_order_id);
}

#[rstest]
fn test_process_cancel_command_order_not_found(
    instrument_eth_usdt: InstrumentAny,
    mut msgbus: MessageBus,
    order_event_handler: ShareableMessageHandler,
) {
    msgbus.register(
        msgbus.switchboard.exec_engine_process,
        order_event_handler.clone(),
    );
    // Create normal l2 engine without reject_stop_orders config param
    let mut engine_l2 = get_order_matching_engine_l2(
        instrument_eth_usdt.clone(),
        Rc::new(RefCell::new(msgbus)),
        None,
        None,
        None,
    );

    let client_order_id = ClientOrderId::from("O-19700101-000000-001-001-1");
    let account_id = AccountId::from("ACCOUNT-001");
    let cancel_command = CancelOrder::new(
        TraderId::from("TRADER-001"),
        ClientId::from("CLIENT-001"),
        StrategyId::from("STRATEGY-001"),
        instrument_eth_usdt.id(),
        client_order_id,
        VenueOrderId::from("V1"),
        UUID4::new(),
        UnixNanos::default(),
    )
    .unwrap();

    // Process cancel command for order which doesn't exists
    engine_l2.process_cancel(&cancel_command, account_id);

    // Check we have received OrderCancelRejected event
    let saved_messages = get_order_event_handler_messages(order_event_handler);
    assert_eq!(saved_messages.len(), 1);
    let order_event = saved_messages.first().unwrap();
    let order_rejected = match order_event {
        OrderEventAny::CancelRejected(order_rejected) => order_rejected,
        _ => panic!("Expected OrderRejected event in first message"),
    };
    assert_eq!(order_rejected.client_order_id, client_order_id);
    assert_eq!(
        order_rejected.reason,
        Ustr::from(format!("Order {client_order_id} not found").as_str())
    );
}

#[rstest]
fn test_process_cancel_all_command(
    instrument_eth_usdt: InstrumentAny,
    mut msgbus: MessageBus,
    order_event_handler: ShareableMessageHandler,
    account_id: AccountId,
) {
    msgbus.register(
        msgbus.switchboard.exec_engine_process,
        order_event_handler.clone(),
    );
    let cache = Rc::new(RefCell::new(Cache::default()));
    let mut engine_l2 = get_order_matching_engine_l2(
        instrument_eth_usdt.clone(),
        Rc::new(RefCell::new(msgbus)),
        Some(cache.clone()),
        None,
        None,
    );

    // Add SELL limit orderbook delta to have ask initialized
    let orderbook_delta_sell = OrderBookDeltaTestBuilder::new(instrument_eth_usdt.id())
        .book_action(BookAction::Add)
        .book_order(BookOrder::new(
            OrderSide::Sell,
            Price::from("1500.00"),
            Quantity::from("1.000"),
            1,
        ))
        .build();
    engine_l2.process_order_book_delta(&orderbook_delta_sell);

    // Create 3 limit orders which are not filled (2 from the same instrument and 1 from different instrument)
    // and update the cache
    // as orders opened from cancel_all command will be read from open_orders cache function
    let client_order_id_1 = ClientOrderId::from("O-19700101-000000-001-001-1");
    let mut limit_order_1 = OrderTestBuilder::new(OrderType::Limit)
        .instrument_id(instrument_eth_usdt.id())
        .side(OrderSide::Buy)
        .price(Price::from("1495.00"))
        .quantity(Quantity::from("1.000"))
        .client_order_id(client_order_id_1)
        .build();
    cache
        .borrow_mut()
        .add_order(limit_order_1.clone(), None, None, false)
        .unwrap();
    engine_l2.process_order(&mut limit_order_1, account_id);
    cache.borrow_mut().update_order(&limit_order_1).unwrap();

    let client_order_id_2 = ClientOrderId::from("O-19700101-000000-001-001-2");
    let mut limit_order_2 = OrderTestBuilder::new(OrderType::Limit)
        .instrument_id(instrument_eth_usdt.id())
        .side(OrderSide::Buy)
        .price(Price::from("1496.00"))
        .quantity(Quantity::from("1.000"))
        .client_order_id(client_order_id_2)
        .build();
    cache
        .borrow_mut()
        .add_order(limit_order_2.clone(), None, None, false)
        .unwrap();
    engine_l2.process_order(&mut limit_order_2, account_id);
    cache.borrow_mut().update_order(&limit_order_2).unwrap();

    let client_order_id_3 = ClientOrderId::from("O-19700101-000000-001-001-3");
    let mut limit_order_3 = OrderTestBuilder::new(OrderType::Limit)
        .instrument_id(InstrumentId::from("BTCUSDT-PERP.BINANCE")) // <-- Different instrument
        .side(OrderSide::Buy)
        .price(Price::from("1497.00"))
        .quantity(Quantity::from("1.000"))
        .client_order_id(client_order_id_3)
        .build();
    cache
        .borrow_mut()
        .add_order(limit_order_3.clone(), None, None, false)
        .unwrap();
    engine_l2.process_order(&mut limit_order_3, account_id);
    cache.borrow_mut().update_order(&limit_order_3).unwrap();

    // Create cancel all order which related to only ETHUSDT-PERP.BINANCE instrument
    let cancel_all_command = CancelAllOrders::new(
        TraderId::from("TRADER-001"),
        ClientId::from("CLIENT-001"),
        StrategyId::from("STRATEGY-001"),
        instrument_eth_usdt.id(),
        OrderSide::Buy,
        UUID4::new(),
        UnixNanos::default(),
    )
    .unwrap();
    engine_l2.process_cancel_all(&cancel_all_command, account_id);

    // Check we have received 3 OrderAccepted and 2 OrderCanceled events
    let saved_messages = get_order_event_handler_messages(order_event_handler);
    assert_eq!(saved_messages.len(), 5);
    let order_event_first = saved_messages.first().unwrap();
    let order_accepted = match order_event_first {
        OrderEventAny::Accepted(order_accepted) => order_accepted,
        _ => panic!("Expected OrderAccepted event in first message"),
    };
    assert_eq!(order_accepted.client_order_id, client_order_id_1);
    let order_event_second = saved_messages.get(1).unwrap();
    let order_accepted = match order_event_second {
        OrderEventAny::Accepted(order_accepted) => order_accepted,
        _ => panic!("Expected OrderAccepted event in second message"),
    };
    assert_eq!(order_accepted.client_order_id, client_order_id_2);
    let order_event_third = saved_messages.get(2).unwrap();
    let order_accepted = match order_event_third {
        OrderEventAny::Accepted(order_accepted) => order_accepted,
        _ => panic!("Expected OrderAccepted event in third message"),
    };
    assert_eq!(order_accepted.client_order_id, client_order_id_3);
    let order_event_fourth = saved_messages.get(3).unwrap();
    let order_canceled_1 = match order_event_fourth {
        OrderEventAny::Canceled(order_canceled) => order_canceled,
        _ => panic!("Expected OrderCanceled event in fourth message"),
    };
    let order_event_fifth = saved_messages.get(4).unwrap();
    let order_canceled_2 = match order_event_fifth {
        OrderEventAny::Canceled(order_canceled) => order_canceled,
        _ => panic!("Expected OrderCanceled event in fifth message"),
    };
    // Because of nondeterministic order of events we need to check both client order ids
    let ids = [
        order_canceled_1.client_order_id,
        order_canceled_2.client_order_id,
    ];
    assert!(ids.contains(&client_order_id_1));
    assert_eq!(order_canceled_1.instrument_id, instrument_eth_usdt.id());
    assert!(ids.contains(&client_order_id_2));
    assert_eq!(order_canceled_2.instrument_id, instrument_eth_usdt.id());
}

#[rstest]
fn test_process_batch_cancel_command(
    instrument_eth_usdt: InstrumentAny,
    mut msgbus: MessageBus,
    order_event_handler: ShareableMessageHandler,
    account_id: AccountId,
) {
    msgbus.register(
        msgbus.switchboard.exec_engine_process,
        order_event_handler.clone(),
    );
    let cache = Rc::new(RefCell::new(Cache::default()));
    let mut engine_l2 = get_order_matching_engine_l2(
        instrument_eth_usdt.clone(),
        Rc::new(RefCell::new(msgbus)),
        Some(cache),
        None,
        None,
    );

    // Add SELL limit orderbook delta to have ask initialized
    let orderbook_delta_sell = OrderBookDeltaTestBuilder::new(instrument_eth_usdt.id())
        .book_action(BookAction::Add)
        .book_order(BookOrder::new(
            OrderSide::Sell,
            Price::from("1500.00"),
            Quantity::from("1.000"),
            1,
        ))
        .build();
    engine_l2.process_order_book_delta(&orderbook_delta_sell);

    // Create 2 limits order which will be canceled with batch cancel command
    let client_order_id_1 = ClientOrderId::from("O-19700101-000000-001-001-1");
    let mut limit_order_1 = OrderTestBuilder::new(OrderType::Limit)
        .instrument_id(instrument_eth_usdt.id())
        .side(OrderSide::Buy)
        .price(Price::from("1495.00"))
        .quantity(Quantity::from("1.000"))
        .client_order_id(client_order_id_1)
        .build();
    let client_order_id_2 = ClientOrderId::from("O-19700101-000000-001-001-2");
    let mut limit_order_2 = OrderTestBuilder::new(OrderType::Limit)
        .instrument_id(instrument_eth_usdt.id())
        .side(OrderSide::Buy)
        .price(Price::from("1496.00"))
        .quantity(Quantity::from("1.000"))
        .client_order_id(client_order_id_2)
        .build();

    engine_l2.process_order(&mut limit_order_1, account_id);
    engine_l2.process_order(&mut limit_order_2, account_id);

    let cancel_1 = CancelOrder::new(
        TraderId::from("TRADER-001"),
        ClientId::from("CLIENT-001"),
        StrategyId::from("STRATEGY-001"),
        instrument_eth_usdt.id(),
        client_order_id_1,
        VenueOrderId::from("V1"),
        UUID4::new(),
        UnixNanos::default(),
    )
    .unwrap();
    let cancel_2 = CancelOrder::new(
        TraderId::from("TRADER-001"),
        ClientId::from("CLIENT-001"),
        StrategyId::from("STRATEGY-001"),
        instrument_eth_usdt.id(),
        client_order_id_2,
        VenueOrderId::from("V2"),
        UUID4::new(),
        UnixNanos::default(),
    )
    .unwrap();
    let batch_cancel_command = BatchCancelOrders::new(
        TraderId::from("TRADER-001"),
        ClientId::from("CLIENT-001"),
        StrategyId::from("STRATEGY-001"),
        instrument_eth_usdt.id(),
        vec![cancel_1, cancel_2],
        UUID4::new(),
        UnixNanos::default(),
    )
    .unwrap();

    engine_l2.process_batch_cancel(&batch_cancel_command, account_id);

    // Check we have received 2 OrderAccepted and 2 OrderCanceled events
    let saved_messages = get_order_event_handler_messages(order_event_handler);
    assert_eq!(saved_messages.len(), 4);
    let order_event_first = saved_messages.first().unwrap();
    let order_accepted = match order_event_first {
        OrderEventAny::Accepted(order_accepted) => order_accepted,
        _ => panic!("Expected OrderAccepted event in first message"),
    };
    assert_eq!(order_accepted.client_order_id, client_order_id_1);
    let order_event_second = saved_messages.get(1).unwrap();
    let order_accepted = match order_event_second {
        OrderEventAny::Accepted(order_accepted) => order_accepted,
        _ => panic!("Expected OrderAccepted event in second message"),
    };
    assert_eq!(order_accepted.client_order_id, client_order_id_2);
    let order_event_third = saved_messages.get(2).unwrap();
    let order_canceled_1 = match order_event_third {
        OrderEventAny::Canceled(order_canceled) => order_canceled,
        _ => panic!("Expected OrderCanceled event in third message"),
    };
    let order_event_fourth = saved_messages.get(3).unwrap();
    let _order_canceled_2 = match order_event_fourth {
        OrderEventAny::Canceled(order_canceled) => order_canceled,
        _ => panic!("Expected OrderCanceled event in fourth message"),
    };
    assert_eq!(order_canceled_1.client_order_id, client_order_id_1);
}

#[rstest]
fn test_expire_order(
    instrument_eth_usdt: InstrumentAny,
    mut msgbus: MessageBus,
    order_event_handler: ShareableMessageHandler,
    account_id: AccountId,
) {
    msgbus.register(
        msgbus.switchboard.exec_engine_process,
        order_event_handler.clone(),
    );

    // Create order matching engine with gtd support
    let mut engine_config = OrderMatchingEngineConfig::default();
    engine_config.support_gtd_orders = true;
    let mut engine_l2 = get_order_matching_engine_l2(
        instrument_eth_usdt.clone(),
        Rc::new(RefCell::new(msgbus)),
        None,
        None,
        Some(engine_config),
    );

    // Add SELL limit orderbook delta to have ask initialized
    let orderbook_delta_sell = OrderBookDeltaTestBuilder::new(instrument_eth_usdt.id())
        .book_action(BookAction::Add)
        .book_order(BookOrder::new(
            OrderSide::Sell,
            Price::from("1500.00"),
            Quantity::from("1.000"),
            1,
        ))
        .build();
    engine_l2.process_order_book_delta(&orderbook_delta_sell);

    // Create GTD LIMIT order which will expire after we process tick
    // that has higher timestamp than expire_time.
    let client_order_id = ClientOrderId::from("O-19700101-000000-001-001-1");
    let expire_time = DateTime::parse_from_rfc3339("2019-10-23T10:32:49.669Z")
        .unwrap()
        .with_timezone(&Utc)
        .timestamp_nanos_opt()
        .unwrap();
    let tick_time = DateTime::parse_from_rfc3339("2025-10-23T10:32:50.000Z")
        .unwrap()
        .with_timezone(&Utc)
        .timestamp_nanos_opt()
        .unwrap();

    let mut limit_order_expire = OrderTestBuilder::new(OrderType::Limit)
        .instrument_id(instrument_eth_usdt.id())
        .side(OrderSide::Buy)
        .price(Price::from("1495.00"))
        .quantity(Quantity::from("1.000"))
        .expire_time(UnixNanos::from(expire_time as u64))
        .client_order_id(client_order_id)
        .build();
    let tick = TradeTick::new(
        instrument_eth_usdt.id(),
        Price::from("1500.00"),
        Quantity::from("1.000"),
        AggressorSide::Buyer,
        TradeId::new("1"),
        UnixNanos::from(tick_time as u64),
        UnixNanos::from(tick_time as u64),
    );
    engine_l2.process_order(&mut limit_order_expire, account_id);
    engine_l2.process_trade_tick(&tick);

    // Check we have received OrderAccepted and then OrderExpired event
    let saved_messages = get_order_event_handler_messages(order_event_handler);
    assert_eq!(saved_messages.len(), 2);
    let order_event_first = saved_messages.first().unwrap();
    let order_accepted = match order_event_first {
        OrderEventAny::Accepted(order_accepted) => order_accepted,
        _ => panic!("Expected OrderAccepted event in first message"),
    };
    assert_eq!(order_accepted.client_order_id, client_order_id);
    let order_event_second = saved_messages.get(1).unwrap();
    let order_expired = match order_event_second {
        OrderEventAny::Expired(order_expired) => order_expired,
        _ => panic!("Expected OrderExpired event in second message"),
    };
    assert_eq!(order_expired.client_order_id, client_order_id);
}

#[rstest]
fn test_process_modify_order_rejected_not_found(
    instrument_eth_usdt: InstrumentAny,
    mut msgbus: MessageBus,
    order_event_handler: ShareableMessageHandler,
    account_id: AccountId,
) {
    msgbus.register(
        msgbus.switchboard.exec_engine_process,
        order_event_handler.clone(),
    );
    let mut engine_l2 = get_order_matching_engine_l2(
        instrument_eth_usdt.clone(),
        Rc::new(RefCell::new(msgbus)),
        None,
        None,
        None,
    );

    // Create modify order command with client order id that didn't pass through the engine
    let client_order_id = ClientOrderId::from("O-19700101-000000-001-001-1");
    let modify_order_command = ModifyOrder::new(
        TraderId::from("TRADER-001"),
        ClientId::from("CLIENT-001"),
        StrategyId::from("STRATEGY-001"),
        instrument_eth_usdt.id(),
        client_order_id,
        VenueOrderId::from("V1"),
        None,
        None,
        None,
        UUID4::new(),
        UnixNanos::default(),
    )
    .unwrap();
    engine_l2.process_modify(&modify_order_command, account_id);

    // Check if we have received OrderModifyRejected event
    let saved_messages = get_order_event_handler_messages(order_event_handler);
    assert_eq!(saved_messages.len(), 1);
    let order_event = saved_messages.first().unwrap();
    let order_rejected = match order_event {
        OrderEventAny::ModifyRejected(order_rejected) => order_rejected,
        _ => panic!("Expected OrderRejected event in first message"),
    };
    assert_eq!(order_rejected.client_order_id, client_order_id);
}

#[rstest]
fn test_update_limit_order_post_only_matched(
    instrument_eth_usdt: InstrumentAny,
    mut msgbus: MessageBus,
    order_event_handler: ShareableMessageHandler,
    account_id: AccountId,
) {
    msgbus.register(
        msgbus.switchboard.exec_engine_process,
        order_event_handler.clone(),
    );
    let mut engine_l2 = get_order_matching_engine_l2(
        instrument_eth_usdt.clone(),
        Rc::new(RefCell::new(msgbus)),
        None,
        None,
        None,
    );

    // Add SELL limit orderbook delta to have ask initialized
    let orderbook_delta_sell = OrderBookDeltaTestBuilder::new(instrument_eth_usdt.id())
        .book_action(BookAction::Add)
        .book_order(BookOrder::new(
            OrderSide::Sell,
            Price::from("1500.00"),
            Quantity::from("1.000"),
            1,
        ))
        .build();
    engine_l2.process_order_book_delta(&orderbook_delta_sell);

    // Create BUY LIMIT order post only which will won't be filled
    let client_order_id = ClientOrderId::from("O-19700101-000000-001-001-1");
    let mut limit_order = OrderTestBuilder::new(OrderType::Limit)
        .instrument_id(instrument_eth_usdt.id())
        .side(OrderSide::Buy)
        .price(Price::from("1495.00"))
        .quantity(Quantity::from("1.000"))
        .client_order_id(client_order_id)
        .post_only(true)
        .build();
    engine_l2.process_order(&mut limit_order, account_id);

    // Create ModifyOrder command to update price of the order to be matched
    let modify_order_command = ModifyOrder::new(
        TraderId::from("TRADER-001"),
        ClientId::from("CLIENT-001"),
        StrategyId::from("STRATEGY-001"),
        instrument_eth_usdt.id(),
        client_order_id,
        VenueOrderId::from("V1"),
        None,
        Some(Price::from("1500.00")), // Set price which will be matched
        None,
        UUID4::new(),
        UnixNanos::default(),
    )
    .unwrap();
    engine_l2.process_modify(&modify_order_command, account_id);

    // Check that we have received OrderAccepted and then OrderModifyRejected event because of post only
    let saved_messages = get_order_event_handler_messages(order_event_handler);
    assert_eq!(saved_messages.len(), 2);
    let order_event_first = saved_messages.first().unwrap();
    let order_accepted = match order_event_first {
        OrderEventAny::Accepted(order_accepted) => order_accepted,
        _ => panic!("Expected OrderAccepted event in first message"),
    };
    assert_eq!(order_accepted.client_order_id, client_order_id);
    let order_event_second = saved_messages.get(1).unwrap();
    let order_rejected = match order_event_second {
        OrderEventAny::ModifyRejected(order_rejected) => order_rejected,
        _ => panic!("Expected OrderModifyRejected event in second message"),
    };
    assert_eq!(order_rejected.client_order_id, client_order_id);
    assert_eq!(
        order_rejected.reason,
        Ustr::from("POST_ONLY LIMIT BUY order with new limit px of 1500.00 would have been a TAKER: bid=None, ask=1500.00")
    );
}

#[rstest]
fn test_update_limit_order_valid(
    instrument_eth_usdt: InstrumentAny,
    mut msgbus: MessageBus,
    order_event_handler: ShareableMessageHandler,
    account_id: AccountId,
) {
    msgbus.register(
        msgbus.switchboard.exec_engine_process,
        order_event_handler.clone(),
    );
    let mut engine_l2 = get_order_matching_engine_l2(
        instrument_eth_usdt.clone(),
        Rc::new(RefCell::new(msgbus)),
        None,
        None,
        None,
    );

    // Add SELL limit orderbook delta to have ask initialized
    let orderbook_delta_sell = OrderBookDeltaTestBuilder::new(instrument_eth_usdt.id())
        .book_action(BookAction::Add)
        .book_order(BookOrder::new(
            OrderSide::Sell,
            Price::from("1500.00"),
            Quantity::from("1.000"),
            1,
        ))
        .build();
    engine_l2.process_order_book_delta(&orderbook_delta_sell);

    // Create BUY LIMIT order post only which will won't be filled
    let client_order_id = ClientOrderId::from("O-19700101-000000-001-001-1");
    let mut limit_order = OrderTestBuilder::new(OrderType::Limit)
        .instrument_id(instrument_eth_usdt.id())
        .side(OrderSide::Buy)
        .price(Price::from("1495.00"))
        .quantity(Quantity::from("1.000"))
        .client_order_id(client_order_id)
        .build();
    engine_l2.process_order(&mut limit_order, account_id);

    // Create ModifyOrder command to update price to 1500.00 where it will be matched immediately
    let new_limit_price = Price::from("1500.00");
    let modify_order_command = ModifyOrder::new(
        TraderId::from("TRADER-001"),
        ClientId::from("CLIENT-001"),
        StrategyId::from("STRATEGY-001"),
        instrument_eth_usdt.id(),
        client_order_id,
        VenueOrderId::from("V1"),
        None,
        Some(new_limit_price),
        None,
        UUID4::new(),
        UnixNanos::default(),
    )
    .unwrap();
    engine_l2.process_modify(&modify_order_command, account_id);

    // Check that we have received OrderAccepted and then OrderUpdated
    // and then OrderFilled as limit order was matched immediately
    let saved_messages = get_order_event_handler_messages(order_event_handler);
    assert_eq!(saved_messages.len(), 3);
    let order_event_first = saved_messages.first().unwrap();
    let order_accepted = match order_event_first {
        OrderEventAny::Accepted(order_accepted) => order_accepted,
        _ => panic!("Expected OrderAccepted event in first message"),
    };
    assert_eq!(order_accepted.client_order_id, client_order_id);
    let order_event_second = saved_messages.get(1).unwrap();
    let order_updated = match order_event_second {
        OrderEventAny::Updated(order_updated) => order_updated,
        _ => panic!("Expected OrderUpdated event in second message"),
    };
    assert_eq!(order_updated.client_order_id, client_order_id);
    assert_eq!(order_updated.price.unwrap(), new_limit_price);
    let order_event_third = saved_messages.get(2).unwrap();
    let order_filled = match order_event_third {
        OrderEventAny::Filled(order_filled) => order_filled,
        _ => panic!("Expected OrderFilled event in third message"),
    };
    assert_eq!(order_filled.client_order_id, client_order_id);
    assert_eq!(order_filled.last_px, new_limit_price);
    assert_eq!(order_filled.last_qty, Quantity::from("1.000"));
}

#[rstest]
fn test_update_stop_market_order_valid(
    instrument_eth_usdt: InstrumentAny,
    mut msgbus: MessageBus,
    order_event_handler: ShareableMessageHandler,
    account_id: AccountId,
) {
    msgbus.register(
        msgbus.switchboard.exec_engine_process,
        order_event_handler.clone(),
    );
    let mut engine_l2 = get_order_matching_engine_l2(
        instrument_eth_usdt.clone(),
        Rc::new(RefCell::new(msgbus)),
        None,
        None,
        None,
    );

    // Add SELL limit orderbook delta to have ask initialized
    let orderbook_delta_sell = OrderBookDeltaTestBuilder::new(instrument_eth_usdt.id())
        .book_action(BookAction::Add)
        .book_order(BookOrder::new(
            OrderSide::Sell,
            Price::from("1500.00"),
            Quantity::from("1.000"),
            1,
        ))
        .build();
    engine_l2.process_order_book_delta(&orderbook_delta_sell);

    // Create BUY STOP MARKET order which is not activated as price of 1495.00 is below current ask of 1500.00
    let client_order_id = ClientOrderId::from("O-19700101-000000-001-001-1");
    let mut stop_market_order = OrderTestBuilder::new(OrderType::StopMarket)
        .instrument_id(instrument_eth_usdt.id())
        .side(OrderSide::Buy)
        .trigger_price(Price::from("1505.00"))
        .quantity(Quantity::from("1.000"))
        .client_order_id(client_order_id)
        .build();
    engine_l2.process_order(&mut stop_market_order, account_id);

    // Create modify command which moves trigger price to 1501.00 which won't trigger the stop price
    //  as ask is at 1500.00 and order will be correctly updated
    let new_trigger_price = Price::from("1501.00");
    let modify_order_command = ModifyOrder::new(
        TraderId::from("TRADER-001"),
        ClientId::from("CLIENT-001"),
        StrategyId::from("STRATEGY-001"),
        instrument_eth_usdt.id(),
        client_order_id,
        VenueOrderId::from("V1"),
        None,
        None,
        Some(new_trigger_price),
        UUID4::new(),
        UnixNanos::default(),
    );
    engine_l2.process_modify(&modify_order_command.unwrap(), account_id);

    // Check that we have received OrderAccepted and then OrderUpdated
    let saved_messages = get_order_event_handler_messages(order_event_handler);
    assert_eq!(saved_messages.len(), 2);
    let order_event_first = saved_messages.first().unwrap();
    let order_accepted = match order_event_first {
        OrderEventAny::Accepted(order_accepted) => order_accepted,
        _ => panic!("Expected OrderAccepted event in first message"),
    };
    assert_eq!(order_accepted.client_order_id, client_order_id);
    let order_event_second = saved_messages.get(1).unwrap();
    println!("{order_event_second:?}");
    let order_updated = match order_event_second {
        OrderEventAny::Updated(order_updated) => order_updated,
        _ => panic!("Expected OrderUpdated event in second message"),
    };
    assert_eq!(order_updated.client_order_id, client_order_id);
    assert_eq!(order_updated.trigger_price.unwrap(), new_trigger_price);
}
