from glob import glob
import os


def add_moltype_args(parser):
    parser.add_argument(
        '--protein', dest='protein', action='store_true',
        help='choose a protein signature; by default, a nucleotide signature is used'
    )
    parser.add_argument(
        '--no-protein', dest='protein', action='store_false',
        help='do not choose a protein signature')
    parser.set_defaults(protein=False)

    parser.add_argument(
        '--dayhoff', dest='dayhoff', action='store_true',
        help='build Dayhoff-encoded amino acid signatures'
    )
    parser.add_argument(
        '--no-dayhoff', dest='dayhoff', action='store_false',
        help='do not build Dayhoff-encoded amino acid signatures')
    parser.set_defaults(dayhoff=False)

    parser.add_argument(
        '--hp', '--hydrophobic-polar', dest='hp', action='store_true',
        help='build hydrophobic-polar-encoded amino acid signatures'
    )
    parser.add_argument(
        '--no-hp', '--no-hydrophobic-polar', dest='hp', action='store_false',
        help='do not build hydrophobic-polar-encoded amino acid signatures')
    parser.set_defaults(hp=False)

    parser.add_argument(
        '--dna', '--rna', dest='dna', default=None, action='store_true',
        help='choose a nucleotide signature (default: True)')
    parser.add_argument(
        '--no-dna', '--no-rna', dest='dna', action='store_false',
        help='do not choose a nucleotide signature')
    parser.set_defaults(dna=None)


def add_construct_moltype_args(parser):
    add_moltype_args(parser)
    parser.set_defaults(dna=True)


def add_ksize_arg(parser, default=31):
    parser.add_argument(
        '-k', '--ksize', metavar='K', default=None, type=int,
        help='k-mer size; default={d}'.format(d=default)
    )

#https://stackoverflow.com/questions/55324449/how-to-specify-a-minimum-or-maximum-float-value-with-argparse#55410582
def range_limited_float_type(arg):
    """ Type function for argparse - a float within some predefined bounds """
    min_val = 0
    max_val = 1
    try:
        f = float(arg)
    except ValueError:
        raise argparse.ArgumentTypeError("Must be a floating point number")
    if f < min_val or f > max_val:
        raise argparse.ArgumentTypeError(f"Argument must be >{str(min_val)} and <{str(max_val)}")
    return f


def add_threshold_arg(parser, default=0.1):
    parser.add_argument(
        '--containment-threshold', default=default, type=range_limited_float_type,
        help=f'minimum containment threshold for classification; default={default}'
    )


def opfilter(path):
    return not path.startswith('__') and path not in ['utils']


def command_list(dirpath):
    paths = glob(os.path.join(dirpath, '*.py'))
    filenames = [os.path.basename(path) for path in paths]
    basenames = [os.path.splitext(path)[0] for path in filenames if not path.startswith('__')]
    basenames = filter(opfilter, basenames)
    return sorted(basenames)