module.exports = {
    plugins: [
        require('postcss-import')({}),
        require('stylelint')({}),
        require('postcss-nesting')({}),
        require('autoprefixer')({})
    ]
}
