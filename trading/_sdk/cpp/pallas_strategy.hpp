#pragma once

#include <algorithm>
#include <cctype>
#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <deque>
#include <functional>
#include <iostream>
#include <limits>
#include <optional>
#include <stdexcept>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

namespace pallas {

struct InstrumentRef {
    std::string exchange;
    std::string symbol;
};

struct Ctx {
    std::string position_qty;
    std::optional<std::string> mid;
    std::string equity;
    std::string balances_json;
    std::string instruments_json;
    std::string pending_orders_json;
    std::string fills_json;
    std::string rejections_json;
    std::string raw_json;
    std::optional<InstrumentRef> instrument;
};

struct Event {
    std::string json;
};

struct Bar {
    double open;
    double high;
    double low;
    double close;
    double volume;
};

struct Intent {
    InstrumentRef instrument;
    std::string side;
    std::string order_type;
    std::string qty;
    std::optional<std::string> price;
    std::optional<std::string> stop_price;
    std::optional<std::string> strategy_id;
    std::optional<std::string> client_order_id;
    std::optional<std::string> oco_group;
};

struct Response {
    std::vector<Intent> intents;
    std::vector<std::string> cancel_order_ids;
    std::vector<std::string> cancel_client_order_ids;
    bool cancel_all = false;
    bool flatten = false;

    Response() = default;
    Response(std::vector<Intent> orders) : intents(std::move(orders)) {}
};

namespace detail {

// This parser only reads the engine-owned protocol schema. If arbitrary JSON becomes part of the
// strategy API, replace it with a full parser instead of extending these field readers.

inline std::size_t skip_space(std::string_view text, std::size_t pos) {
    while (pos < text.size() && std::isspace(static_cast<unsigned char>(text[pos]))) {
        ++pos;
    }
    return pos;
}

inline std::optional<std::size_t> value_pos(std::string_view json, std::string_view key) {
    const std::string needle = "\"" + std::string(key) + "\"";
    std::size_t pos = json.find(needle);
    while (pos != std::string_view::npos) {
        pos = skip_space(json, pos + needle.size());
        if (pos < json.size() && json[pos] == ':') {
            return skip_space(json, pos + 1);
        }
        pos = json.find(needle, pos + 1);
    }
    return std::nullopt;
}

inline std::string parse_string(std::string_view json, std::size_t pos) {
    if (pos >= json.size() || json[pos] != '"') {
        throw std::runtime_error("expected JSON string");
    }
    std::string value;
    for (++pos; pos < json.size(); ++pos) {
        const char ch = json[pos];
        if (ch == '"') {
            return value;
        }
        if (ch != '\\') {
            value.push_back(ch);
            continue;
        }
        if (++pos >= json.size()) {
            break;
        }
        switch (json[pos]) {
            case '"': value.push_back('"'); break;
            case '\\': value.push_back('\\'); break;
            case '/': value.push_back('/'); break;
            case 'b': value.push_back('\b'); break;
            case 'f': value.push_back('\f'); break;
            case 'n': value.push_back('\n'); break;
            case 'r': value.push_back('\r'); break;
            case 't': value.push_back('\t'); break;
            default: throw std::runtime_error("unsupported JSON escape in protocol field");
        }
    }
    throw std::runtime_error("unterminated JSON string");
}

inline std::string string_field(std::string_view json, std::string_view key) {
    const auto pos = value_pos(json, key);
    if (!pos) {
        throw std::runtime_error("missing protocol field: " + std::string(key));
    }
    return parse_string(json, *pos);
}

inline std::optional<std::string> optional_string_field(
    std::string_view json,
    std::string_view key
) {
    const auto pos = value_pos(json, key);
    if (!pos || json.substr(*pos, 4) == "null") {
        return std::nullopt;
    }
    return parse_string(json, *pos);
}

inline std::optional<double> optional_number_field(
    std::string_view json,
    std::string_view key
) {
    const auto pos = value_pos(json, key);
    if (!pos || json.substr(*pos, 4) == "null") {
        return std::nullopt;
    }
    const std::string value = json[*pos] == '"'
        ? parse_string(json, *pos)
        : std::string(json.substr(*pos));
    std::size_t parsed = 0;
    const double number = std::stod(value, &parsed);
    if (parsed == 0 || !std::isfinite(number)) {
        throw std::runtime_error("invalid numeric protocol field: " + std::string(key));
    }
    return number;
}

inline std::uint64_t uint_field(std::string_view json, std::string_view key) {
    const auto pos = value_pos(json, key);
    if (!pos) {
        throw std::runtime_error("missing protocol field: " + std::string(key));
    }
    std::uint64_t value = 0;
    std::size_t cursor = *pos;
    if (cursor >= json.size() || !std::isdigit(static_cast<unsigned char>(json[cursor]))) {
        throw std::runtime_error("invalid unsigned protocol field: " + std::string(key));
    }
    while (cursor < json.size() && std::isdigit(static_cast<unsigned char>(json[cursor]))) {
        const auto digit = static_cast<std::uint64_t>(json[cursor] - '0');
        if (value > (std::numeric_limits<std::uint64_t>::max() - digit) / 10) {
            throw std::runtime_error("unsigned protocol field overflow: " + std::string(key));
        }
        value = value * 10 + digit;
        ++cursor;
    }
    return value;
}

inline std::string object_field(std::string_view json, std::string_view key) {
    const auto pos = value_pos(json, key);
    if (!pos || *pos >= json.size() || json[*pos] != '{') {
        throw std::runtime_error("missing protocol object: " + std::string(key));
    }
    std::size_t depth = 0;
    bool in_string = false;
    bool escaped = false;
    for (std::size_t cursor = *pos; cursor < json.size(); ++cursor) {
        const char ch = json[cursor];
        if (in_string) {
            if (escaped) {
                escaped = false;
            } else if (ch == '\\') {
                escaped = true;
            } else if (ch == '"') {
                in_string = false;
            }
            continue;
        }
        if (ch == '"') {
            in_string = true;
        } else if (ch == '{') {
            ++depth;
        } else if (ch == '}' && --depth == 0) {
            return std::string(json.substr(*pos, cursor - *pos + 1));
        }
    }
    throw std::runtime_error("unterminated protocol object: " + std::string(key));
}

inline std::string array_field(std::string_view json, std::string_view key) {
    const auto pos = value_pos(json, key);
    if (!pos || *pos >= json.size() || json[*pos] != '[') {
        throw std::runtime_error("missing protocol array: " + std::string(key));
    }
    std::size_t depth = 0;
    bool in_string = false;
    bool escaped = false;
    for (std::size_t cursor = *pos; cursor < json.size(); ++cursor) {
        const char ch = json[cursor];
        if (in_string) {
            if (escaped) {
                escaped = false;
            } else if (ch == '\\') {
                escaped = true;
            } else if (ch == '"') {
                in_string = false;
            }
            continue;
        }
        if (ch == '"') {
            in_string = true;
        } else if (ch == '[') {
            ++depth;
        } else if (ch == ']' && --depth == 0) {
            return std::string(json.substr(*pos, cursor - *pos + 1));
        }
    }
    throw std::runtime_error("unterminated protocol array: " + std::string(key));
}

inline std::string escape(std::string_view value) {
    static constexpr char hex[] = "0123456789abcdef";
    std::string out;
    out.reserve(value.size() + 2);
    for (const unsigned char ch : value) {
        switch (ch) {
            case '"': out += "\\\""; break;
            case '\\': out += "\\\\"; break;
            case '\b': out += "\\b"; break;
            case '\f': out += "\\f"; break;
            case '\n': out += "\\n"; break;
            case '\r': out += "\\r"; break;
            case '\t': out += "\\t"; break;
            default:
                if (ch < 0x20) {
                    out += "\\u00";
                    out.push_back(hex[ch >> 4]);
                    out.push_back(hex[ch & 0x0f]);
                } else {
                    out.push_back(static_cast<char>(ch));
                }
        }
    }
    return out;
}

inline void write_string(std::ostream& out, std::string_view key, std::string_view value) {
    out << '"' << key << "\":\"" << escape(value) << '"';
}

}  // namespace detail

inline Ctx context_from_json(std::string_view json) {
    return Ctx{
        detail::string_field(json, "position_qty"),
        detail::optional_string_field(json, "mid"),
        detail::string_field(json, "equity"),
        detail::object_field(json, "balances"),
        detail::array_field(json, "instruments"),
        detail::array_field(json, "pending_orders"),
        detail::array_field(json, "fills"),
        detail::array_field(json, "rejections"),
        std::string(json),
        std::nullopt,
    };
}

inline void write_response(
    std::uint64_t seq,
    const Response& response
) {
    std::cout << "{\"msg\":\"intents\",\"seq\":" << seq << ",\"intents\":[";
    for (std::size_t index = 0; index < response.intents.size(); ++index) {
        if (index != 0) {
            std::cout << ',';
        }
        const auto& intent = response.intents[index];
        std::cout << "{\"instrument\":{";
        detail::write_string(std::cout, "exchange", intent.instrument.exchange);
        std::cout << ',';
        detail::write_string(std::cout, "symbol", intent.instrument.symbol);
        std::cout << "},";
        detail::write_string(std::cout, "side", intent.side);
        std::cout << ',';
        detail::write_string(std::cout, "order_type", intent.order_type);
        std::cout << ',';
        detail::write_string(std::cout, "qty", intent.qty);
        std::cout << ",\"price\":";
        if (intent.price) {
            std::cout << '"' << detail::escape(*intent.price) << '"';
        } else {
            std::cout << "null";
        }
        std::cout << ",\"stop_price\":";
        if (intent.stop_price) {
            std::cout << '"' << detail::escape(*intent.stop_price) << '"';
        } else {
            std::cout << "null";
        }
        std::cout << ",\"strategy_id\":";
        if (intent.strategy_id) {
            std::cout << '"' << detail::escape(*intent.strategy_id) << '"';
        } else {
            std::cout << "null";
        }
        std::cout << ",\"client_order_id\":";
        if (intent.client_order_id) {
            std::cout << '"' << detail::escape(*intent.client_order_id) << '"';
        } else {
            std::cout << "null";
        }
        std::cout << ",\"oco_group\":";
        if (intent.oco_group) {
            std::cout << '"' << detail::escape(*intent.oco_group) << '"';
        } else {
            std::cout << "null";
        }
        std::cout << '}';
    }
    std::cout << "],\"cancel_order_ids\":[";
    for (std::size_t index = 0; index < response.cancel_order_ids.size(); ++index) {
        if (index != 0) std::cout << ',';
        std::cout << '"' << detail::escape(response.cancel_order_ids[index]) << '"';
    }
    std::cout << "],\"cancel_client_order_ids\":[";
    for (std::size_t index = 0; index < response.cancel_client_order_ids.size(); ++index) {
        if (index != 0) std::cout << ',';
        std::cout << '"' << detail::escape(response.cancel_client_order_ids[index]) << '"';
    }
    std::cout << "],\"cancel_all\":" << (response.cancel_all ? "true" : "false")
              << ",\"flatten\":" << (response.flatten ? "true" : "false") << "}\n";
    std::cout.flush();
}

inline void write_intents(std::uint64_t seq, const std::vector<Intent>& intents) {
    write_response(seq, Response{intents});
}

inline std::optional<Bar> bar_from_event(const Event& event) {
    if (event.json.find("\"Market\"") == std::string::npos ||
        event.json.find("\"Bar\"") == std::string::npos) {
        return std::nullopt;
    }
    const auto open = detail::optional_number_field(event.json, "open");
    const auto high = detail::optional_number_field(event.json, "high");
    const auto low = detail::optional_number_field(event.json, "low");
    const auto close = detail::optional_number_field(event.json, "close");
    const auto volume = detail::optional_number_field(event.json, "volume");
    if (!open || !high || !low || !close || !volume) {
        return std::nullopt;
    }
    return Bar{*open, *high, *low, *close, *volume};
}

inline std::optional<double> bar_close(const Event& event) {
    const auto bar = bar_from_event(event);
    return bar ? std::optional<double>(bar->close) : std::nullopt;
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

inline double position_size_pct_equity(double equity, double mid, double pct = 0.1) {
    if (mid <= 0.0) {
        return 0.0;
    }
    return std::max((equity * pct) / mid, 0.0);
}

using EventHandler = std::function<Response(const Ctx&, const Event&)>;
using InitHandler = std::function<void(std::string_view)>;
using FinishHandler = std::function<Response(const Ctx&)>;

inline void run(
    EventHandler on_event,
    const InitHandler& on_init = nullptr,
    const FinishHandler& on_finish = nullptr
) {
    std::string line;
    if (!std::getline(std::cin, line)) {
        return;
    }
    if (detail::string_field(line, "msg") != "init") {
        throw std::runtime_error("expected init message");
    }
    const InstrumentRef instrument{
        detail::string_field(line, "exchange"),
        detail::string_field(line, "symbol"),
    };
    if (on_init) {
        on_init(line);
    }
    std::cout << "{\"msg\":\"ready\",\"capabilities\":[\"finish\"]}\n";
    std::cout.flush();

    while (std::getline(std::cin, line)) {
        if (line.empty()) {
            continue;
        }
        const std::string message = detail::string_field(line, "msg");
        if (message == "shutdown") {
            break;
        }
        if (message == "finish") {
            Ctx context = context_from_json(detail::object_field(line, "ctx"));
            context.instrument = instrument;
            write_response(
                detail::uint_field(line, "seq"),
                on_finish ? on_finish(context) : Response{}
            );
            continue;
        }
        if (message != "event") {
            continue;
        }
        Ctx context = context_from_json(detail::object_field(line, "ctx"));
        context.instrument = instrument;
        const Event event{detail::object_field(line, "event")};
        write_response(detail::uint_field(line, "seq"), on_event(context, event));
    }
}

}  // namespace pallas
