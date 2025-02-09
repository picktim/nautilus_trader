# -------------------------------------------------------------------------------------------------
#  Copyright (C) 2015-2025 Nautech Systems Pty Ltd. All rights reserved.
#  https://nautechsystems.io
#
#  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
#  You may not use this file except in compliance with the License.
#  You may obtain a copy of the License at https://www.gnu.org/licenses/lgpl-3.0.en.html
#
#  Unless required by applicable law or agreed to in writing, software
#  distributed under the License is distributed on an "AS IS" BASIS,
#  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
#  See the License for the specific language governing permissions and
#  limitations under the License.
# -------------------------------------------------------------------------------------------------

import asyncio
from operator import attrgetter
from typing import Any

import pandas as pd

# fmt: off
from nautilus_trader.adapters.interactive_brokers.client import InteractiveBrokersClient
from nautilus_trader.adapters.interactive_brokers.common import IB_VENUE
from nautilus_trader.adapters.interactive_brokers.common import IBContract
from nautilus_trader.adapters.interactive_brokers.config import InteractiveBrokersDataClientConfig
from nautilus_trader.adapters.interactive_brokers.parsing.data import timedelta_to_duration_str
from nautilus_trader.adapters.interactive_brokers.providers import InteractiveBrokersInstrumentProvider
from nautilus_trader.cache.cache import Cache
from nautilus_trader.common.component import LiveClock
from nautilus_trader.common.component import MessageBus
from nautilus_trader.data.messages import RequestBars
from nautilus_trader.data.messages import RequestData
from nautilus_trader.data.messages import RequestInstruments
from nautilus_trader.data.messages import RequestQuoteTicks
from nautilus_trader.data.messages import RequestTradeTicks
from nautilus_trader.live.data_client import LiveMarketDataClient
from nautilus_trader.model.data import Bar
from nautilus_trader.model.data import BarType
from nautilus_trader.model.data import DataType
from nautilus_trader.model.data import QuoteTick
from nautilus_trader.model.data import TradeTick
from nautilus_trader.model.enums import BookType
from nautilus_trader.model.identifiers import ClientId
from nautilus_trader.model.identifiers import InstrumentId
from nautilus_trader.model.instruments.currency_pair import CurrencyPair


# fmt: on


class InteractiveBrokersDataClient(LiveMarketDataClient):
    """
    Provides a data client for the InteractiveBrokers exchange by using the `Gateway` to
    stream market data.

    Parameters
    ----------
    loop : asyncio.AbstractEventLoop
        The event loop for the client.
    client : InteractiveBrokersClient
        The nautilus InteractiveBrokersClient using ibapi.
    msgbus : MessageBus
        The message bus for the client.
    cache : Cache
        The cache for the client.
    clock : LiveClock
        The clock for the client.
    instrument_provider : InteractiveBrokersInstrumentProvider
        The instrument provider.
    ibg_client_id : int
        Client ID used to connect TWS/Gateway.
    config : InteractiveBrokersDataClientConfig
        Configuration for the client.
    name : str, optional
        The custom client ID.

    """

    def __init__(
        self,
        loop: asyncio.AbstractEventLoop,
        client: InteractiveBrokersClient,
        msgbus: MessageBus,
        cache: Cache,
        clock: LiveClock,
        instrument_provider: InteractiveBrokersInstrumentProvider,
        ibg_client_id: int,
        config: InteractiveBrokersDataClientConfig,
        name: str | None = None,
        connection_timeout: int = 300,
        request_timeout: int = 60,
    ) -> None:
        super().__init__(
            loop=loop,
            client_id=ClientId(name or f"{IB_VENUE.value}-{ibg_client_id:03d}"),
            venue=None,
            msgbus=msgbus,
            cache=cache,
            clock=clock,
            instrument_provider=instrument_provider,
            config=config,
        )
        self._connection_timeout = connection_timeout
        self._request_timeout = request_timeout
        self._client = client
        self._handle_revised_bars = config.handle_revised_bars
        self._use_regular_trading_hours = config.use_regular_trading_hours
        self._market_data_type = config.market_data_type
        self._ignore_quote_tick_size_updates = config.ignore_quote_tick_size_updates

    @property
    def instrument_provider(self) -> InteractiveBrokersInstrumentProvider:
        return self._instrument_provider  # type: ignore

    async def _connect(self):
        # Connect client
        await self._client.wait_until_ready(self._connection_timeout)
        self._client.registered_nautilus_clients.add(self.id)

        # Set Market Data Type
        await self._client.set_market_data_type(self._market_data_type)

        # Load instruments based on config
        await self.instrument_provider.initialize()
        for instrument in self._instrument_provider.list_all():
            self._handle_data(instrument)

    async def _disconnect(self):
        self._client.registered_nautilus_clients.remove(self.id)
        if self._client.is_running and self._client.registered_nautilus_clients == set():
            self._client.stop()

    async def _subscribe(self, data_type: DataType, params: dict[str, Any] | None = None) -> None:
        raise NotImplementedError(  # pragma: no cover
            "implement the `_subscribe` coroutine",  # pragma: no cover
        )

    async def _subscribe_instruments(self, params: dict[str, Any] | None = None) -> None:
        raise NotImplementedError(  # pragma: no cover
            "implement the `_subscribe_instruments` coroutine",  # pragma: no cover
        )

    async def _subscribe_instrument(
        self,
        instrument_id: InstrumentId,
        params: dict[str, Any] | None = None,
    ) -> None:
        raise NotImplementedError(  # pragma: no cover
            "implement the `_subscribe_instrument` coroutine",  # pragma: no cover
        )

    async def _subscribe_order_book_deltas(
        self,
        instrument_id: InstrumentId,
        book_type: BookType,
        depth: int | None = None,
        params: dict[str, Any] | None = None,
    ) -> None:
        raise NotImplementedError(  # pragma: no cover
            "implement the `_subscribe_order_book_deltas` coroutine",  # pragma: no cover
        )

    async def _subscribe_order_book_snapshots(
        self,
        instrument_id: InstrumentId,
        book_type: BookType,
        depth: int | None = None,
        params: dict[str, Any] | None = None,
    ) -> None:
        raise NotImplementedError(  # pragma: no cover
            "implement the `_subscribe_order_book_snapshots` coroutine",  # pragma: no cover
        )

    async def _subscribe_quote_ticks(
        self,
        instrument_id: InstrumentId,
        params: dict[str, Any] | None = None,
    ) -> None:
        if not (instrument := self._cache.instrument(instrument_id)):
            self._log.error(
                f"Cannot subscribe to quotes for {instrument_id}: instrument not found",
            )
            return

        await self._client.subscribe_ticks(
            instrument_id=instrument_id,
            contract=IBContract(**instrument.info["contract"]),
            tick_type="BidAsk",
            ignore_size=self._ignore_quote_tick_size_updates,
        )

    async def _subscribe_trade_ticks(
        self,
        instrument_id: InstrumentId,
        params: dict[str, Any] | None = None,
    ) -> None:
        if not (instrument := self._cache.instrument(instrument_id)):
            self._log.error(
                f"Cannot subscribe to trades for {instrument_id}: instrument not found",
            )
            return

        if isinstance(instrument, CurrencyPair):
            self._log.error(
                "Interactive Brokers does not support trades for CurrencyPair instruments",
            )
            return

        await self._client.subscribe_ticks(
            instrument_id=instrument_id,
            contract=IBContract(**instrument.info["contract"]),
            tick_type="AllLast",
            ignore_size=self._ignore_quote_tick_size_updates,
        )

    async def _subscribe_bars(
        self,
        bar_type: BarType,
        params: dict[str, Any] | None = None,
    ) -> None:
        if not (instrument := self._cache.instrument(bar_type.instrument_id)):
            self._log.error(f"Cannot subscribe to {bar_type} bars: instrument not found")
            return

        if bar_type.spec.timedelta.total_seconds() == 5:
            await self._client.subscribe_realtime_bars(
                bar_type=bar_type,
                contract=IBContract(**instrument.info["contract"]),
                use_rth=self._use_regular_trading_hours,
            )
        else:
            await self._client.subscribe_historical_bars(
                bar_type=bar_type,
                contract=IBContract(**instrument.info["contract"]),
                use_rth=self._use_regular_trading_hours,
                handle_revised_bars=self._handle_revised_bars,
            )

    async def _subscribe_instrument_status(
        self,
        instrument_id: InstrumentId,
        params: dict[str, Any] | None = None,
    ) -> None:
        pass  # Subscribed as part of orderbook

    async def _subscribe_instrument_close(
        self,
        instrument_id: InstrumentId,
        params: dict[str, Any] | None = None,
    ) -> None:
        pass  # Subscribed as part of orderbook

    async def _unsubscribe(self, data_type: DataType, params: dict[str, Any] | None = None) -> None:
        raise NotImplementedError(  # pragma: no cover
            "implement the `_unsubscribe` coroutine",  # pragma: no cover
        )

    async def _unsubscribe_instruments(self, params: dict[str, Any] | None = None) -> None:
        raise NotImplementedError(  # pragma: no cover
            "implement the `_unsubscribe_instruments` coroutine",  # pragma: no cover
        )

    async def _unsubscribe_instrument(
        self,
        instrument_id: InstrumentId,
        params: dict[str, Any] | None = None,
    ) -> None:
        raise NotImplementedError(  # pragma: no cover
            "implement the `_unsubscribe_instrument` coroutine",  # pragma: no cover
        )

    async def _unsubscribe_order_book_deltas(
        self,
        instrument_id: InstrumentId,
        params: dict[str, Any] | None = None,
    ) -> None:
        raise NotImplementedError(  # pragma: no cover
            "implement the `_unsubscribe_order_book_deltas` coroutine",  # pragma: no cover
        )

    async def _unsubscribe_order_book_snapshots(
        self,
        instrument_id: InstrumentId,
        params: dict[str, Any] | None = None,
    ) -> None:
        raise NotImplementedError(  # pragma: no cover
            "implement the `_unsubscribe_order_book_snapshots` coroutine",  # pragma: no cover
        )

    async def _unsubscribe_quote_ticks(
        self,
        instrument_id: InstrumentId,
        params: dict[str, Any] | None = None,
    ) -> None:
        await self._client.unsubscribe_ticks(instrument_id, "BidAsk")

    async def _unsubscribe_trade_ticks(
        self,
        instrument_id: InstrumentId,
        params: dict[str, Any] | None = None,
    ) -> None:
        await self._client.unsubscribe_ticks(instrument_id, "AllLast")

    async def _unsubscribe_bars(
        self,
        bar_type: BarType,
        params: dict[str, Any] | None = None,
    ) -> None:
        if bar_type.spec.timedelta == 5:
            await self._client.unsubscribe_realtime_bars(bar_type)
        else:
            await self._client.unsubscribe_historical_bars(bar_type)

    async def _unsubscribe_instrument_status(
        self,
        instrument_id: InstrumentId,
        params: dict[str, Any] | None = None,
    ) -> None:
        pass  # Subscribed as part of orderbook

    async def _unsubscribe_instrument_close(
        self,
        instrument_id: InstrumentId,
        params: dict[str, Any] | None = None,
    ) -> None:
        pass  # Subscribed as part of orderbook

    async def _request(self, request: RequestData) -> None:
        raise NotImplementedError(  # pragma: no cover
            "implement the `_request` coroutine",  # pragma: no cover
        )

    async def _request_instrument(self, request: RequestInstruments) -> None:
        if request.start is not None:
            self._log.warning(
                f"Requesting instrument {request.instrument_id} with specified `start` which has no effect",
            )

        if request.end is not None:
            self._log.warning(
                f"Requesting instrument {request.instrument_id} with specified `end` which has no effect",
            )

        await self.instrument_provider.load_async(request.instrument_id)
        if instrument := self.instrument_provider.find(request.instrument_id):
            self._handle_data(instrument)
        else:
            self._log.warning(f"Instrument for {request.instrument_id} not available")
            return

        self._handle_instrument(instrument, request.id, request.params)

    async def _request_instruments(self, request: RequestInstruments) -> None:
        raise NotImplementedError(  # pragma: no cover
            "implement the `_request_instruments` coroutine",  # pragma: no cover
        )

    async def _request_quote_ticks(self, request: RequestQuoteTicks) -> None:
        if not (instrument := self._cache.instrument(request.instrument_id)):
            self._log.error(
                f"Cannot request quotes for {request.instrument_id}, instrument not found",
            )
            return

        ticks = await self._handle_ticks_request(
            IBContract(**instrument.info["contract"]),
            "BID_ASK",
            request.limit,
            request.start,
            request.end,
        )
        if not ticks:
            self._log.warning(f"No quote tick data received for {request.instrument_id}")
            return

        self._handle_quote_ticks(request.instrument_id, ticks, request.id, request.params)

    async def _request_trade_ticks(self, request: RequestTradeTicks) -> None:
        if not (instrument := self._cache.instrument(request.instrument_id)):
            self._log.error(
                f"Cannot request trades for {request.instrument_id}: instrument not found",
            )
            return

        if isinstance(instrument, CurrencyPair):
            self._log.error(
                "Interactive Brokers does not support trades for CurrencyPair instruments",
            )
            return

        ticks = await self._handle_ticks_request(
            IBContract(**instrument.info["contract"]),
            "TRADES",
            request.limit,
            request.start,
            request.end,
        )
        if not ticks:
            self._log.warning(f"No trades received for {request.instrument_id}")
            return

        self._handle_trade_ticks(request.instrument_id, ticks, request.id, request.params)

    async def _handle_ticks_request(
        self,
        contract: IBContract,
        tick_type: str,
        limit: int,
        start: pd.Timestamp | None = None,
        end: pd.Timestamp | None = None,
    ) -> list[QuoteTick | TradeTick]:
        if not start:
            limit = self._cache.tick_capacity

        if not end:
            end = pd.Timestamp.utcnow()

        ticks: list[QuoteTick | TradeTick] = []
        while (start and end > start) or (len(ticks) < limit > 0):
            await self._client.wait_until_ready()
            ticks_part = await self._client.get_historical_ticks(
                contract,
                tick_type,
                end_date_time=end,
                use_rth=self._use_regular_trading_hours,
                timeout=self._request_timeout,
            )
            if not ticks_part:
                break
            end = pd.Timestamp(min(ticks_part, key=attrgetter("ts_init")).ts_init, tz="UTC")
            ticks.extend(ticks_part)

        ticks.sort(key=lambda x: x.ts_init)
        return ticks

    async def _request_bars(self, request: RequestBars) -> None:
        if not (instrument := self._cache.instrument(request.bar_type.instrument_id)):
            self._log.error(f"Cannot request {request.bar_type} bars: instrument not found")
            return

        if not request.bar_type.spec.is_time_aggregated():
            self._log.error(
                f"Cannot request {request.bar_type} bars: only time bars are aggregated by Interactive Brokers",
            )
            return

        limit = request.limit
        if not request.start and limit == 0:
            limit = 1000

        end = request.end
        if not request.end:
            end = pd.Timestamp.utcnow()

        if request.start:
            duration = end - request.start
            duration_str = timedelta_to_duration_str(duration)
        else:
            duration_str = "7 D" if request.bar_type.spec.timedelta.total_seconds() >= 60 else "1 D"

        bars: list[Bar] = []
        while (request.start and end > request.start) or (len(bars) < limit > 0):
            bars_part: list[Bar] = await self._client.get_historical_bars(
                bar_type=request.bar_type,
                contract=IBContract(**instrument.info["contract"]),
                use_rth=self._use_regular_trading_hours,
                end_date_time=end,
                duration=duration_str,
                timeout=self._request_timeout,
            )
            bars.extend(bars_part)
            if not bars_part or request.start:
                break
            end = pd.Timestamp(min(bars, key=attrgetter("ts_event")).ts_event, tz="UTC")

        if bars:
            bars = list(set(bars))
            bars.sort(key=lambda x: x.ts_init)
            self._handle_bars(request.bar_type, bars, bars[0], request.id, request.params)
            status_msg = {"id": request.id, "status": "Success"}
        else:
            self._log.warning(f"No bar data received for {request.bar_type}")
            status_msg = {"id": request.id, "status": "Failed"}

        # Publish Status event
        self._msgbus.publish(
            topic=f"requests.{request.id}",
            msg=status_msg,
        )
