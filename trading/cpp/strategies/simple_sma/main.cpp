#include "../../sdk/pallas_strategy.hpp"

#include <cstdlib>
#include <string>
#include <vector>

namespace {

constexpr int kFast = 5;
constexpr int kSlow = 20;
constexpr const char* kQty = "0.01";

pallas::RollingSma g_fast(kFast);
pallas::RollingSma g_slow(kSlow);
int g_prev_sign = 0;

std::vector<pallas::Intent> on_event(const pallas::Ctx& ctx, const pallas::json& event) {
    std::vector<pallas::Intent> intents;
    const auto close = pallas::bar_close(event);
    if (!close) {
        return intents;
    }

    const auto f = g_fast.update(*close);
    const auto s = g_slow.update(*close);
    if (!f || !s) {
        return intents;
    }

    int sign = 0;
    if (*f > *s) {
        sign = 1;
    } else if (*f < *s) {
        sign = -1;
    }

    if (sign != 0 && sign != g_prev_sign) {
        pallas::Intent intent;
        intent.instrument = {"binance", "BTCUSDT"};
        intent.order_type = "Market";
        if (sign > 0) {
            intent.side = "Buy";
            intent.qty = kQty;
            intents.push_back(intent);
        } else {
            const double pos = std::stod(ctx.position_qty);
            if (pos > 0.0) {
                intent.side = "Sell";
                intent.qty = ctx.position_qty;
                intents.push_back(intent);
            }
        }
    }

    if (sign != 0) {
        g_prev_sign = sign;
    }
    return intents;
}

}  // namespace

int main() {
    try {
        pallas::run(on_event);
    } catch (const std::exception& ex) {
        std::cerr << "strategy error: " << ex.what() << '\n';
        return 1;
    }
    return 0;
}
