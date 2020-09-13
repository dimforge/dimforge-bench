const webpack = require('webpack');
const path = require("path");
const CopyPlugin = require("copy-webpack-plugin");

const dist = path.resolve(__dirname, "dist");

const mode = "production";

const appConfig = {
    mode: mode,
    entry: "./src/index.js",
    devServer: {
        contentBase: dist
    },
    resolve: {
        extensions: [".js"]
    },
    output: {
        path: dist,
        filename: "index.js"
    },
    plugins: [
        new CopyPlugin([
            path.resolve(__dirname, "static")
        ]),
        new webpack.IgnorePlugin(/(fs)/)
    ]
};

module.exports = [appConfig];
