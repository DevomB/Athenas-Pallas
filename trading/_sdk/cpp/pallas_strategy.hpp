#pragma once

// JSON protocol helpers for external strategies (stdin/stdout line JSON).
// Vendored nlohmann/json single header: json.hpp (v3.11.3)

#include "json.hpp"

#include <cmath>
#include <cstdlib>
#include <deque>
#include <functional>
#include <iostream>
#include <optional>
#include <string>
#include <vector>

namespace pallas {

using json = nlohmann::json;

struct Ctx {
    std::string position_qty;
    std::optional<std::string> mid;
    std::string equity;
    json balances;
};

struct InstrumentRef {
    std::string exchange;
    std::string symbol;
};

struct Intent {
    InstrumentRef instrument;
    std::string side;
    std::string order_type;
    std::string qty;
    std::optional<std::string> price;
};

inline Ctx ctx_from_json(const json& j) {
    Ctx c;
    c.position_qty = j.at("position_qty").get<std::string>();
    if (j.contains("mid") && !j.at("mid").is_null()) {
        c.mid = j.at("mid").get<std::string>();
    }
    c.equity = j.at("equity").get<std::string>();
    c.balances = j.at("balances");
    return c;
}

inline json read_line() {
    std::string line;
    if (!std::getline(std::cin, line)) {
        std::exit(0);
    }
    return json::parse(line);
}

inline void write_intents(std::uint64_t seq, const std::vector<Intent>& intents) {
    json out;
    out["msg"] = "intents";
    out["seq"] = seq;
    out["intents"] = json::array();
    for (const auto& i : intents) {
        json item;
        item["instrument"] = {{"exchange", i.instrument.exchange}, {"symbol", i.instrument.symbol}};
        item["side"] = i.side;
        item["order_type"] = i.order_type;
        item["qty"] = i.qty;
        if (i.price) {
            item["price"] = *i.price;
        } else {
            item["price"] = nullptr;
        }
        out["intents"].push_back(std::move(item));
    }
    std::cout << out.dump() << '\n';
    std::cout.flush();
}

inline std::optional<double> bar_close(const json& event) {
    if (!event.contains("Market") || !event.at("Market").is_object()) {
        return std::nullopt;
    }
    const json& market = event.at("Market");
    if (!market.contains("Bar") || !market.at("Bar").is_object()) {
        return std::nullopt;
    }
    const json& bar = market.at("Bar");
    if (!bar.contains("close")) {
        return std::nullopt;
    }
    const json& close = bar.at("close");
    if (close.is_string()) {
        return std::stod(close.get<std::string>());
    }
    if (close.is_number()) {
        return close.get<double>();
    }
    return std::nullopt;
}

class RollingSma {
  public:
    explicit RollingSma(std::size_t window) : window_(window) {}

    std::optional<double> update(double value) {
        if (buf_.size() == window_) {
            total_ -= buf_.front();
            buf_.pop_front();
        }
        buf_.push_back(value);
        total_ += value;
        if (buf_.size() < window_) {
            return std::nullopt;
        }
        return total_ / static_cast<double>(window_);
    }

  private:
    std::size_t window_;
    std::deque<double> buf_;
    double total_ = 0.0;
};

// Base qty for `pct` of mark-to-market equity at `mid` (spot-style). Returns 0 for non-positive mid.
inline double position_size_pct_equity(double equity, double mid, double pct = 0.1) {
    if (mid <= 0.0) {
        return 0.0;
    }
    double qty = (equity * pct) / mid;
    return qty > 0.0 ? qty : 0.0;
}

using EventHandler = std::function<std::vector<Intent>(const Ctx&, const json&)>;

inline void run(EventHandler on_event, const std::function<void(const json&)>& on_init = nullptr) {
    json init = read_line();
    if (!init.contains("msg") || init.at("msg").get<std::string>() != "init") {
        throw std::runtime_error("expected init message");
    }
    if (on_init) {
        on_init(init);
    }
    std::cout << R"({"msg":"ready"})" << '\n';
    std::cout.flush();

    while (true) {
        std::string line;
        if (!std::getline(std::cin, line)) {
            break;
        }
        if (line.empty()) {
            continue;
        }
        json msg = json::parse(line);
        if (msg.contains("msg") && msg.at("msg").get<std::string>() == "shutdown") {
            break;
        }
        if (!msg.contains("msg") || msg.at("msg").get<std::string>() != "event") {
            continue;
        }
        const Ctx ctx = ctx_from_json(msg.at("ctx"));
        const json& event = msg.at("event");
        const std::uint64_t seq = msg.at("seq").get<std::uint64_t>();
        std::vector<Intent> intents = on_event(ctx, event);
        write_intents(seq, intents);
    }
}

}  // namespace pallas
